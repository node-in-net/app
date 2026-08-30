use common::AppError;
pub use nodeinnet_p2p::p2p::SysInfo;
use nodeinnet_p2p::P2pMessage;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Default)]
pub struct SysInfoCache {
    latest: Mutex<HashMap<String, SysInfo>>,
    waiting: Mutex<HashMap<String, oneshot::Sender<SysInfo>>>,
}

impl SysInfoCache {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn register(&self, resource_id: &str) -> oneshot::Receiver<SysInfo> {
        let (tx, rx) = oneshot::channel();
        self.waiting
            .lock()
            .unwrap()
            .insert(resource_id.to_string(), tx);
        rx
    }

    pub fn latest(&self, resource_id: &str) -> Option<SysInfo> {
        self.latest.lock().unwrap().get(resource_id).cloned()
    }

    pub fn forget(&self, resource_id: &str) {
        self.latest.lock().unwrap().remove(resource_id);
        self.waiting.lock().unwrap().remove(resource_id);
    }

    pub fn route(&self, msg: P2pMessage) -> Option<P2pMessage> {
        match msg {
            P2pMessage::SystemInfoResponse { resource_id, info } => {
                self.latest
                    .lock()
                    .unwrap()
                    .insert(resource_id.clone(), info.clone());
                if let Some(tx) = self.waiting.lock().unwrap().remove(&resource_id) {
                    let _ = tx.send(info);
                }
                None
            }
            other => Some(other),
        }
    }
}

pub struct SysInfoRpc {
    resource_id: String,
    cache: Arc<SysInfoCache>,
    send: Box<dyn Fn(P2pMessage)>,
    timeout: Duration,
}

impl SysInfoRpc {
    pub fn new(
        resource_id: impl Into<String>,
        cache: Arc<SysInfoCache>,
        send: impl Fn(P2pMessage) + 'static,
    ) -> Self {
        Self {
            resource_id: resource_id.into(),
            cache,
            send: Box::new(send),
            timeout: Duration::from_secs(5),
        }
    }

    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub async fn fetch(&self) -> Result<SysInfo, AppError> {
        let rx = self.cache.register(&self.resource_id);
        (self.send)(P2pMessage::RequestSystemInfo {
            resource_id: self.resource_id.clone(),
        });
        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(info)) => Ok(info),
            Ok(Err(_)) => Err(AppError::Other("request dropped".into())),
            Err(_) => {
                self.cache.forget(&self.resource_id);
                Err(AppError::Other("peer did not answer in time".into()))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    SystemInfoChanged { info: SysInfo },
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SystemInfoState {
    pub wired: bool,
    pub last: Option<SysInfo>,
    pub last_error: Option<String>,
}

#[derive(Default)]
pub struct SystemInfo {
    rpc: Option<SysInfoRpc>,
    events: Vec<Event>,
    last: Option<SysInfo>,
    last_error: Option<String>,
}

impl SystemInfo {
    pub fn wire(&mut self, rpc: SysInfoRpc) {
        self.rpc = Some(rpc);
    }

    pub fn unwire(&mut self) {
        self.rpc = None;
        self.last = None;
        self.last_error = None;
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn state(&self) -> SystemInfoState {
        SystemInfoState {
            wired: self.rpc.is_some(),
            last: self.last.clone(),
            last_error: self.last_error.clone(),
        }
    }

    pub async fn refresh(&mut self) {
        let Some(rpc) = &self.rpc else {
            self.last_error = Some("this device does not share system information".into());
            return;
        };
        match rpc.fetch().await {
            Ok(info) => {
                self.last_error = None;
                self.last = Some(info.clone());
                self.events.push(Event::SystemInfoChanged { info });
            }
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(host: &str) -> SysInfo {
        SysInfo {
            hostname: host.into(),
            os_family: "linux".into(),
            os_type: "Arch Linux".into(),
            os_version: "N/A".into(),
            cpu_arch: "x86_64".into(),
            cpu_cores: 8,
            cpu_usage: 12.5,
            total_memory: 16 * 1024 * 1024 * 1024,
            used_memory: 4 * 1024 * 1024 * 1024,
            total_swap: 0,
            used_swap: 0,
            uptime: 1234,
            network_interfaces: vec!["eth0: 10.0.0.2".into()],
        }
    }

    fn response(resource: &str, host: &str) -> P2pMessage {
        P2pMessage::SystemInfoResponse {
            resource_id: resource.into(),
            info: snapshot(host),
        }
    }

    #[test]
    fn a_snapshot_is_claimed_and_everything_else_passes_through() {
        let cache = SysInfoCache::new();
        assert!(
            cache.route(response("sysinfo-a", "box-a")).is_none(),
            "nothing further down the chain reads this message"
        );
        assert!(cache.route(P2pMessage::Ping(1)).is_some());
    }

    #[test]
    fn one_peers_snapshot_does_not_disturb_another() {
        let cache = SysInfoCache::new();
        cache.route(response("sysinfo-a", "box-a"));
        cache.route(response("sysinfo-b", "box-b"));
        assert_eq!(cache.latest("sysinfo-a").unwrap().hostname, "box-a");
        assert_eq!(cache.latest("sysinfo-b").unwrap().hostname, "box-b");
    }

    #[tokio::test]
    async fn fetch_returns_the_answer_that_arrives_afterwards() {
        let cache = SysInfoCache::new();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let rpc = {
            let cache = cache.clone();
            let seen = seen.clone();
            SysInfoRpc::new("sysinfo-a", cache.clone(), move |msg| {
                if let P2pMessage::RequestSystemInfo { resource_id } = msg {
                    seen.lock().unwrap().push(resource_id.clone());
                    cache.route(response(&resource_id, "box-a"));
                }
            })
        };

        let info = rpc.fetch().await.expect("the answer resolves the waiter");
        assert_eq!(info.hostname, "box-a");
        assert_eq!(*seen.lock().unwrap(), vec!["sysinfo-a".to_string()]);
    }

    #[tokio::test]
    async fn fetch_gives_up_when_the_peer_says_nothing() {
        let cache = SysInfoCache::new();
        let mut rpc = SysInfoRpc::new("sysinfo-a", cache, |_| {});
        rpc.timeout = Duration::from_millis(50);
        assert!(
            rpc.fetch().await.is_err(),
            "a silent peer must not hang the panel"
        );
    }

    #[test]
    fn an_unasked_snapshot_still_lands() {
        let cache = SysInfoCache::new();
        cache.route(response("sysinfo-a", "box-a"));
        assert!(cache.latest("sysinfo-a").is_some());
        cache.forget("sysinfo-a");
        assert!(cache.latest("sysinfo-a").is_none());
    }

    #[tokio::test]
    async fn an_unwired_device_reports_rather_than_waiting() {
        let mut sys = SystemInfo::default();
        sys.refresh().await;
        assert!(sys.state().last.is_none());
        assert!(sys.state().last_error.is_some());
        assert!(sys.take_events().is_empty());
    }

    #[tokio::test]
    async fn switching_device_drops_the_previous_reading() {
        let cache = SysInfoCache::new();
        let mut sys = SystemInfo::default();
        {
            let cache = cache.clone();
            sys.wire(SysInfoRpc::new("sysinfo-a", cache.clone(), move |msg| {
                if let P2pMessage::RequestSystemInfo { resource_id } = msg {
                    cache.route(response(&resource_id, "box-a"));
                }
            }));
        }
        sys.refresh().await;
        assert_eq!(sys.state().last.unwrap().hostname, "box-a");
        assert_eq!(sys.take_events().len(), 1);

        sys.unwire();
        assert!(
            sys.state().last.is_none(),
            "the previous device's reading must not sit under the new device's name"
        );
    }
}
