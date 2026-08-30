use common::AppError;
use nodeinnet_p2p::p2p::{LaunchableApp, RemoteAppSession};
use nodeinnet_p2p::P2pMessage;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

use crate::remote_fs::PendingP2p;

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RemoteAppsView {
    pub apps: Vec<LaunchableApp>,
    pub sessions: Vec<RemoteAppSession>,
    pub refused: Option<String>,
}

pub struct RemoteAppsRpc {
    resource_id: String,
    pending: Arc<PendingP2p>,
    send: Box<dyn Fn(P2pMessage)>,
    timeout: Duration,
}

impl RemoteAppsRpc {
    pub fn new(
        resource_id: impl Into<String>,
        pending: Arc<PendingP2p>,
        send: impl Fn(P2pMessage) + 'static,
    ) -> Self {
        Self {
            resource_id: resource_id.into(),
            pending,
            send: Box::new(send),
            timeout: Duration::from_secs(10),
        }
    }

    async fn call(&self, request_id: Uuid, msg: P2pMessage) -> Result<P2pMessage, AppError> {
        let rx = self.pending.register(request_id);
        (self.send)(msg);
        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => Err(AppError::Other("request dropped".into())),
            Err(_) => {
                self.pending.forget(&request_id);
                Err(AppError::Other("peer did not answer in time".into()))
            }
        }
    }

    pub async fn list(&self) -> Result<RemoteAppsView, AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::AppListRequest {
            resource_id: self.resource_id.clone(),
            request_id,
        };
        match self.call(request_id, req).await? {
            P2pMessage::AppListResponse {
                apps,
                sessions,
                refused,
                ..
            } => Ok(RemoteAppsView {
                apps,
                sessions,
                refused,
            }),
            other => Err(AppError::Other(format!("unexpected reply: {other:?}"))),
        }
    }

    pub async fn launch(&self, app_id: &str, session_id: Uuid) -> Result<(), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::AppLaunchRequest {
            resource_id: self.resource_id.clone(),
            request_id,
            session_id,
            app_id: app_id.to_string(),
        };
        self.action(request_id, req).await
    }

    pub async fn stop(&self, session_id: Uuid) -> Result<(), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::AppStopRequest {
            resource_id: self.resource_id.clone(),
            request_id,
            session_id,
        };
        self.action(request_id, req).await
    }

    async fn action(&self, request_id: Uuid, req: P2pMessage) -> Result<(), AppError> {
        match self.call(request_id, req).await? {
            P2pMessage::AppActionResponse { error: None, .. } => Ok(()),
            P2pMessage::AppActionResponse {
                error: Some(code), ..
            } => Err(AppError::Other(code)),
            other => Err(AppError::Other(format!("unexpected reply: {other:?}"))),
        }
    }
}

#[derive(Default)]
pub struct RemoteAppsCache {
    by_resource: Mutex<HashMap<String, RemoteAppsView>>,
}

impl RemoteAppsCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn put(&self, resource_id: &str, view: RemoteAppsView) {
        self.by_resource
            .lock()
            .unwrap()
            .insert(resource_id.to_string(), view);
    }

    pub fn get(&self, resource_id: &str) -> Option<RemoteAppsView> {
        self.by_resource.lock().unwrap().get(resource_id).cloned()
    }

    pub fn forget(&self, resource_id: &str) {
        self.by_resource.lock().unwrap().remove(resource_id);
    }

    pub fn route(&self, msg: P2pMessage) -> Option<P2pMessage> {
        match msg {
            P2pMessage::AppListResponse {
                request_id: None,
                ref resource_id,
                ref apps,
                ref sessions,
                ref refused,
                ..
            } => {
                self.put(
                    resource_id,
                    RemoteAppsView {
                        apps: apps.clone(),
                        sessions: sessions.clone(),
                        refused: refused.clone(),
                    },
                );
                None
            }
            other => Some(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(resource: &str, app: &str) -> P2pMessage {
        P2pMessage::AppListResponse {
            resource_id: resource.into(),
            request_id: None,
            apps: vec![LaunchableApp {
                id: app.into(),
                name: app.into(),
                icon_name: None,
            }],
            sessions: Vec::new(),
            refused: None,
            event: Some("exited".into()),
        }
    }

    #[test]
    fn one_peers_push_does_not_disturb_another() {
        let cache = RemoteAppsCache::new();
        cache.route(push("network-a", "firefox"));
        cache.route(push("network-b", "thunderbird"));

        assert_eq!(cache.get("network-a").unwrap().apps[0].id, "firefox");
        assert_eq!(cache.get("network-b").unwrap().apps[0].id, "thunderbird");
    }

    #[test]
    fn a_push_is_claimed_and_everything_else_passes_through() {
        let cache = RemoteAppsCache::new();
        assert!(
            cache.route(push("network-a", "firefox")).is_none(),
            "the push is ours: nobody is waiting for it"
        );

        let solicited = P2pMessage::AppListResponse {
            resource_id: "network-a".into(),
            request_id: Some(Uuid::new_v4()),
            apps: Vec::new(),
            sessions: Vec::new(),
            refused: None,
            event: None,
        };
        assert!(cache.route(solicited).is_some());
        assert!(cache.route(P2pMessage::Ping(1)).is_some());
    }

    #[test]
    fn a_forgotten_peer_leaves_nothing_behind() {
        let cache = RemoteAppsCache::new();
        cache.route(push("network-a", "firefox"));
        cache.forget("network-a");
        assert!(cache.get("network-a").is_none());
    }
}
