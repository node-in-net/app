use nodeinnet_p2p::P2pMessage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebVpnTotals {
    pub requests: u64,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub streams_opened: u64,
    pub streams_closed: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Connected {
        is_success: bool,
        error_msg: Option<String>,
    },
    Data(Vec<u8>),
    Closed,
}

#[derive(Default)]
struct RouterInner {
    http: HashMap<Uuid, mpsc::UnboundedSender<P2pMessage>>,
    streams: HashMap<Uuid, mpsc::UnboundedSender<StreamEvent>>,
    totals: WebVpnTotals,
}

pub struct WebVpnRouter {
    inner: Mutex<RouterInner>,
}

impl std::fmt::Debug for WebVpnRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebVpnRouter").finish_non_exhaustive()
    }
}

impl WebVpnRouter {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(RouterInner::default()),
        })
    }

    pub fn route(&self, msg: P2pMessage) -> Option<P2pMessage> {
        let mut inner = self.inner.lock().unwrap();
        match msg {
            P2pMessage::HttpResponseStart { request_id, .. }
            | P2pMessage::HttpResponseChunk { request_id, .. }
            | P2pMessage::HttpResponseComplete { request_id, .. }
                if inner.http.contains_key(&request_id) =>
            {
                if let P2pMessage::HttpResponseChunk { chunk, .. } = &msg {
                    inner.totals.bytes_down += chunk.len() as u64;
                }
                let done = matches!(msg, P2pMessage::HttpResponseComplete { .. });
                if let Some(tx) = inner.http.get(&request_id) {
                    let _ = tx.send(msg);
                }
                if done {
                    inner.http.remove(&request_id);
                }
                None
            }
            P2pMessage::SocksConnectResponse {
                stream_id,
                is_success,
                error_msg,
                ..
            } if inner.streams.contains_key(&stream_id) => {
                if let Some(tx) = inner.streams.get(&stream_id) {
                    let _ = tx.send(StreamEvent::Connected {
                        is_success,
                        error_msg,
                    });
                }
                if !is_success {
                    inner.streams.remove(&stream_id);
                    inner.totals.errors += 1;
                }
                None
            }
            P2pMessage::SocksData {
                stream_id, data, ..
            } if inner.streams.contains_key(&stream_id) => {
                inner.totals.bytes_down += data.len() as u64;
                if let Some(tx) = inner.streams.get(&stream_id) {
                    let _ = tx.send(StreamEvent::Data(data));
                }
                None
            }
            P2pMessage::SocksClose { stream_id, .. } if inner.streams.contains_key(&stream_id) => {
                if let Some(tx) = inner.streams.remove(&stream_id) {
                    let _ = tx.send(StreamEvent::Closed);
                }
                inner.totals.streams_closed += 1;
                None
            }
            other => Some(other),
        }
    }

    pub fn totals(&self) -> WebVpnTotals {
        self.inner.lock().unwrap().totals.clone()
    }

    fn register_http(&self, id: Uuid) -> mpsc::UnboundedReceiver<P2pMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut inner = self.inner.lock().unwrap();
        inner.http.insert(id, tx);
        inner.totals.requests += 1;
        rx
    }

    fn forget_http(&self, id: &Uuid) {
        self.inner.lock().unwrap().http.remove(id);
    }

    pub fn register_stream(&self, id: Uuid) -> mpsc::UnboundedReceiver<StreamEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut inner = self.inner.lock().unwrap();
        inner.streams.insert(id, tx);
        inner.totals.streams_opened += 1;
        rx
    }

    pub fn close_stream(&self, id: Uuid) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.streams.remove(&id).is_some() {
            inner.totals.streams_closed += 1;
            true
        } else {
            false
        }
    }

    pub fn count_up(&self, n: usize) {
        self.inner.lock().unwrap().totals.bytes_up += n as u64;
    }

    fn count_error(&self) {
        self.inner.lock().unwrap().totals.errors += 1;
    }

    fn drain_streams(&self) -> Vec<Uuid> {
        let mut inner = self.inner.lock().unwrap();
        let ids: Vec<Uuid> = inner.streams.keys().copied().collect();
        for (_, tx) in inner.streams.drain() {
            let _ = tx.send(StreamEvent::Closed);
        }
        inner.totals.streams_closed += ids.len() as u64;
        ids
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FetchResponse {
    pub status: u16,
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebVpnState {
    pub active: bool,
    pub resource_id: Option<String>,
    pub totals: WebVpnTotals,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    WebvpnChanged { webvpn: WebVpnState },
}

type Transport = (String, Box<dyn Fn(P2pMessage)>);

pub struct WebVpn {
    router: Arc<WebVpnRouter>,
    transport: Option<Transport>,
    active: bool,
    stream_rx: HashMap<Uuid, mpsc::UnboundedReceiver<StreamEvent>>,
    events: Vec<Event>,
    timeout: Duration,
}

impl Default for WebVpn {
    fn default() -> Self {
        Self::with_router(WebVpnRouter::new())
    }
}

impl WebVpn {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_router(router: Arc<WebVpnRouter>) -> Self {
        Self {
            router,
            transport: None,
            active: false,
            stream_rx: HashMap::new(),
            events: Vec::new(),
            timeout: Duration::from_secs(15),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn router(&self) -> Arc<WebVpnRouter> {
        self.router.clone()
    }

    pub fn set_resource(
        &mut self,
        resource_id: impl Into<String>,
        send: impl Fn(P2pMessage) + 'static,
    ) {
        if self.active {
            self.stop();
        }
        self.transport = Some((resource_id.into(), Box::new(send)));
        self.emit();
    }

    pub fn start(&mut self) -> bool {
        if self.transport.is_none() {
            return false;
        }
        self.active = true;
        self.emit();
        true
    }

    pub fn stop(&mut self) -> bool {
        if !self.active {
            return false;
        }
        for stream_id in self.router.drain_streams() {
            if let Some((resource_id, send)) = &self.transport {
                send(P2pMessage::SocksClose {
                    resource_id: resource_id.clone(),
                    stream_id,
                });
            }
        }
        self.stream_rx.clear();
        self.active = false;
        self.emit();
        true
    }

    pub fn unwire(&mut self) {
        self.stop();
        if self.transport.is_none() {
            return;
        }
        self.transport = None;
        self.emit();
    }

    pub fn state(&self) -> WebVpnState {
        WebVpnState {
            active: self.active,
            resource_id: self.transport.as_ref().map(|(id, _)| id.clone()),
            totals: self.router.totals(),
        }
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn emit(&mut self) {
        self.events.push(Event::WebvpnChanged {
            webvpn: self.state(),
        });
    }

    pub async fn fetch(
        &mut self,
        method: &str,
        url: &str,
        headers: std::collections::BTreeMap<String, String>,
        body: Option<Vec<u8>>,
    ) -> Result<FetchResponse, String> {
        if !self.active {
            return Err("webvpn is not active".into());
        }
        let Some((resource_id, send)) = &self.transport else {
            return Err("webvpn is not wired".into());
        };
        let request_id = Uuid::new_v4();
        let mut rx = self.router.register_http(request_id);
        send(P2pMessage::HttpRequest {
            resource_id: resource_id.clone(),
            request_id,
            method: method.to_string(),
            url: url.to_string(),
            headers,
            body,
        });

        let mut resp = FetchResponse {
            status: 0,
            headers: Default::default(),
            body: Vec::new(),
        };
        loop {
            match tokio::time::timeout(self.timeout, rx.recv()).await {
                Ok(Some(P2pMessage::HttpResponseStart {
                    status, headers, ..
                })) => {
                    resp.status = status;
                    resp.headers = headers;
                }
                Ok(Some(P2pMessage::HttpResponseChunk { chunk, .. })) => {
                    resp.body.extend_from_slice(&chunk);
                }
                Ok(Some(P2pMessage::HttpResponseComplete { .. })) => return Ok(resp),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => {
                    self.router.forget_http(&request_id);
                    self.router.count_error();
                    self.emit();
                    return Err("peer did not answer in time".into());
                }
            }
        }
    }

    pub async fn connect_stream(&mut self, host: &str, port: u16) -> Result<Uuid, String> {
        if !self.active {
            return Err("webvpn is not active".into());
        }
        let Some((resource_id, send)) = &self.transport else {
            return Err("webvpn is not wired".into());
        };
        let stream_id = Uuid::new_v4();
        let mut rx = self.router.register_stream(stream_id);
        send(P2pMessage::SocksConnectRequest {
            resource_id: resource_id.clone(),
            stream_id,
            host: host.to_string(),
            port,
        });
        match tokio::time::timeout(self.timeout, rx.recv()).await {
            Ok(Some(StreamEvent::Connected {
                is_success: true, ..
            })) => {
                self.stream_rx.insert(stream_id, rx);
                Ok(stream_id)
            }
            Ok(Some(StreamEvent::Connected { error_msg, .. })) => {
                Err(error_msg.unwrap_or_else(|| "peer could not connect".into()))
            }
            _ => {
                self.router.count_error();
                Err("peer did not answer in time".into())
            }
        }
    }

    pub fn stream_send(&mut self, stream_id: Uuid, data: Vec<u8>) -> bool {
        if !self.stream_rx.contains_key(&stream_id) {
            return false;
        }
        let Some((resource_id, send)) = &self.transport else {
            return false;
        };
        self.router.count_up(data.len());
        send(P2pMessage::SocksData {
            resource_id: resource_id.clone(),
            stream_id,
            data,
        });
        true
    }

    pub async fn stream_recv(&mut self, stream_id: Uuid) -> Result<StreamEvent, String> {
        let Some(rx) = self.stream_rx.get_mut(&stream_id) else {
            return Err("unknown stream".into());
        };
        match tokio::time::timeout(self.timeout, rx.recv()).await {
            Ok(Some(ev)) => {
                if ev == StreamEvent::Closed {
                    self.stream_rx.remove(&stream_id);
                }
                Ok(ev)
            }
            Ok(None) => {
                self.stream_rx.remove(&stream_id);
                Ok(StreamEvent::Closed)
            }
            Err(_) => Err("stream stalled".into()),
        }
    }

    pub fn close_stream(&mut self, stream_id: Uuid) -> bool {
        if self.stream_rx.remove(&stream_id).is_none() {
            return false;
        }
        {
            let mut inner = self.router.inner.lock().unwrap();
            if inner.streams.remove(&stream_id).is_some() {
                inner.totals.streams_closed += 1;
            }
        }
        if let Some((resource_id, send)) = &self.transport {
            send(P2pMessage::SocksClose {
                resource_id: resource_id.clone(),
                stream_id,
            });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Sent = Rc<RefCell<Vec<P2pMessage>>>;

    fn rig(answer: impl Fn(&P2pMessage) -> Vec<P2pMessage> + 'static) -> (WebVpn, Sent) {
        let sent: Sent = Rc::new(RefCell::new(Vec::new()));
        let mut v = WebVpn::new().with_timeout(Duration::from_millis(200));
        let router = v.router();
        let s = sent.clone();
        v.set_resource("net-1", move |msg| {
            for resp in answer(&msg) {
                assert!(router.route(resp).is_none(), "reply must be claimed");
            }
            s.borrow_mut().push(msg);
        });
        v.start();
        v.take_events();
        (v, sent)
    }

    fn http_ok(req: &P2pMessage) -> Vec<P2pMessage> {
        if let P2pMessage::HttpRequest {
            resource_id,
            request_id,
            ..
        } = req
        {
            vec![
                P2pMessage::HttpResponseStart {
                    resource_id: resource_id.clone(),
                    request_id: *request_id,
                    status: 200,
                    headers: [("x-egress".to_string(), "peer".to_string())].into(),
                },
                P2pMessage::HttpResponseChunk {
                    resource_id: resource_id.clone(),
                    request_id: *request_id,
                    chunk: b"pon".to_vec(),
                },
                P2pMessage::HttpResponseChunk {
                    resource_id: resource_id.clone(),
                    request_id: *request_id,
                    chunk: b"g".to_vec(),
                },
                P2pMessage::HttpResponseComplete {
                    resource_id: resource_id.clone(),
                    request_id: *request_id,
                },
            ]
        } else {
            vec![]
        }
    }

    #[tokio::test]
    async fn fetch_collects_the_streamed_response() {
        let (mut v, _) = rig(http_ok);
        let resp = v
            .fetch("GET", "http://x/ping", Default::default(), None)
            .await
            .unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"pong");
        assert_eq!(resp.headers["x-egress"], "peer");
        let t = v.state().totals;
        assert_eq!(t.requests, 1);
        assert_eq!(t.bytes_down, 4);
        assert_eq!(t.errors, 0);
    }

    #[tokio::test]
    async fn fetch_timeout_is_an_error_and_leaks_nothing() {
        let (mut v, _) = rig(|_| vec![]);
        let err = v
            .fetch("GET", "http://x/", Default::default(), None)
            .await
            .unwrap_err();
        assert!(err.contains("did not answer"));
        let t = v.state().totals;
        assert_eq!(t.errors, 1);
        let stray = P2pMessage::HttpResponseComplete {
            resource_id: "net-1".into(),
            request_id: Uuid::new_v4(),
        };
        assert!(v.router().route(stray).is_some());
    }

    #[tokio::test]
    async fn fetch_requires_an_active_tunnel() {
        let (mut v, _) = rig(http_ok);
        v.stop();
        assert!(v
            .fetch("GET", "http://x/", Default::default(), None)
            .await
            .is_err());
    }

    fn socks_echo(req: &P2pMessage) -> Vec<P2pMessage> {
        match req {
            P2pMessage::SocksConnectRequest {
                resource_id,
                stream_id,
                ..
            } => {
                vec![P2pMessage::SocksConnectResponse {
                    resource_id: resource_id.clone(),
                    stream_id: *stream_id,
                    is_success: true,
                    error_msg: None,
                }]
            }
            P2pMessage::SocksData {
                resource_id,
                stream_id,
                data,
            } => {
                vec![P2pMessage::SocksData {
                    resource_id: resource_id.clone(),
                    stream_id: *stream_id,
                    data: data.clone(),
                }]
            }
            _ => vec![],
        }
    }

    #[tokio::test]
    async fn socks_stream_round_trips_and_accounts_bytes() {
        let (mut v, _) = rig(socks_echo);
        let id = v.connect_stream("example.test", 80).await.unwrap();
        assert!(v.stream_send(id, b"hello".to_vec()));
        assert_eq!(
            v.stream_recv(id).await.unwrap(),
            StreamEvent::Data(b"hello".to_vec())
        );
        let t = v.state().totals;
        assert_eq!((t.streams_opened, t.bytes_up, t.bytes_down), (1, 5, 5));

        assert!(v.close_stream(id));
        assert_eq!(v.state().totals.streams_closed, 1);
        assert!(
            !v.stream_send(id, b"late".to_vec()),
            "closed stream refuses input"
        );
    }

    #[tokio::test]
    async fn refused_connect_surfaces_the_peer_error() {
        let (mut v, _) = rig(|req| {
            if let P2pMessage::SocksConnectRequest {
                resource_id,
                stream_id,
                ..
            } = req
            {
                vec![P2pMessage::SocksConnectResponse {
                    resource_id: resource_id.clone(),
                    stream_id: *stream_id,
                    is_success: false,
                    error_msg: Some("connection refused".into()),
                }]
            } else {
                vec![]
            }
        });
        let err = v.connect_stream("example.test", 81).await.unwrap_err();
        assert!(err.contains("refused"));
        assert_eq!(v.state().totals.errors, 1);
    }

    #[tokio::test]
    async fn unwire_stops_and_drops_the_peer() {
        let (mut v, _) = rig(socks_echo);
        v.unwire();
        assert!(!v.state().active);
        assert!(v.state().resource_id.is_none());
        assert!(!v.start(), "an unwired tunnel has nothing to start");
    }

    #[tokio::test]
    async fn stop_closes_every_live_stream() {
        let (mut v, sent) = rig(socks_echo);
        let a = v.connect_stream("x", 1).await.unwrap();
        let b = v.connect_stream("y", 2).await.unwrap();
        assert!(v.stop());
        let closes = sent
            .borrow()
            .iter()
            .filter(|m| matches!(m, P2pMessage::SocksClose { .. }))
            .count();
        assert_eq!(closes, 2, "SocksClose sent for every live stream");
        assert_eq!(v.state().totals.streams_closed, 2);
        assert!(!v.stream_send(a, vec![1]));
        assert!(!v.stream_send(b, vec![1]));
    }

    #[test]
    fn unrelated_messages_are_handed_back() {
        let router = WebVpnRouter::new();
        assert!(router.route(P2pMessage::Goodbye).is_some());
        let stray = P2pMessage::SocksData {
            resource_id: "r".into(),
            stream_id: Uuid::new_v4(),
            data: vec![1],
        };
        assert!(
            router.route(stray).is_some(),
            "unknown stream is not swallowed"
        );
    }
}
