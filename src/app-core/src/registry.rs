use crate::remote_fs::PendingP2p;
use common::AppError;
pub use nodeinnet_p2p::p2p::{RegistryValueData, RegistryValueInfo};
use nodeinnet_p2p::P2pMessage;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub struct RemotePeerRegistryRpc {
    resource_id: String,
    pending: Arc<PendingP2p>,
    send: Box<dyn Fn(P2pMessage)>,
    timeout: Duration,
}

impl RemotePeerRegistryRpc {
    pub fn new(
        resource_id: impl Into<String>,
        pending: Arc<PendingP2p>,
        send: impl Fn(P2pMessage) + 'static,
    ) -> Self {
        Self {
            resource_id: resource_id.into(),
            pending,
            send: Box::new(send),
            timeout: Duration::from_secs(15),
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

    pub async fn request_keys(
        &self,
        path: &str,
    ) -> Result<(Vec<String>, Vec<RegistryValueInfo>), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::RequestRegistryKeys {
            request_id,
            resource_id: self.resource_id.clone(),
            path: path.to_string(),
        };
        match self.call(request_id, req).await? {
            P2pMessage::RegistryKeysResponse {
                subkeys,
                values,
                error,
                ..
            } => match error {
                Some(e) => Err(AppError::Other(e)),
                None => Ok((subkeys, values)),
            },
            _ => Err(unexpected("RegistryKeysResponse")),
        }
    }

    pub async fn set_value(
        &self,
        path: &str,
        value_name: &str,
        value_data: RegistryValueData,
    ) -> Result<(), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::SetRegistryValueRequest {
            request_id,
            resource_id: self.resource_id.clone(),
            path: path.to_string(),
            value_name: value_name.to_string(),
            value_data,
        };
        match self.call(request_id, req).await? {
            P2pMessage::SetRegistryValueResponse { result, .. } => result.map_err(AppError::Other),
            _ => Err(unexpected("SetRegistryValueResponse")),
        }
    }

    pub async fn create_key(&self, parent_path: &str, key_name: &str) -> Result<(), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::CreateRegistryKeyRequest {
            request_id,
            resource_id: self.resource_id.clone(),
            parent_path: parent_path.to_string(),
            key_name: key_name.to_string(),
        };
        match self.call(request_id, req).await? {
            P2pMessage::CreateRegistryKeyResponse { result, .. } => result.map_err(AppError::Other),
            _ => Err(unexpected("CreateRegistryKeyResponse")),
        }
    }

    pub async fn delete_entry(
        &self,
        path: &str,
        value_name: Option<String>,
        is_key: bool,
    ) -> Result<(), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::DeleteRegistryEntryRequest {
            request_id,
            resource_id: self.resource_id.clone(),
            path: path.to_string(),
            value_name,
            is_key,
        };
        match self.call(request_id, req).await? {
            P2pMessage::DeleteRegistryEntryResponse { result, .. } => {
                result.map_err(AppError::Other)
            }
            _ => Err(unexpected("DeleteRegistryEntryResponse")),
        }
    }
}

fn parent_of(path: &str) -> Option<String> {
    let trimmed = path.trim_matches('/');
    let (parent, _) = trimmed.rsplit_once('/')?;
    Some(parent.to_string())
}

fn unexpected(wanted: &str) -> AppError {
    AppError::Other(format!("peer sent an unexpected reply (wanted {wanted})"))
}

#[derive(Default)]
pub struct Registry {
    rpc: Option<RemotePeerRegistryRpc>,
    events: Vec<Event>,
    last: Option<Snapshot>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Snapshot {
    pub path: String,
    pub subkeys: Vec<String>,
    pub values: Vec<RegistryValueInfo>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RegistryState {
    pub wired: bool,
    pub last: Option<Snapshot>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    RegistryChanged {
        path: String,
        subkeys: Vec<String>,
        values: Vec<RegistryValueInfo>,
    },
}

impl Registry {
    pub fn wire(&mut self, rpc: RemotePeerRegistryRpc) {
        self.rpc = Some(rpc);
    }

    pub fn unwire(&mut self) {
        self.rpc = None;
    }

    pub async fn request_keys(&mut self, path: String) {
        let Some(rpc) = &self.rpc else {
            self.last_error = Some("no peer registry is wired".into());
            return;
        };
        match rpc.request_keys(&path).await {
            Ok((subkeys, values)) => {
                self.last_error = None;
                self.last = Some(Snapshot {
                    path: path.clone(),
                    subkeys: subkeys.clone(),
                    values: values.clone(),
                });
                self.events.push(Event::RegistryChanged {
                    path,
                    subkeys,
                    values,
                });
            }
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    pub async fn set_value(&mut self, path: String, value_name: String, data: RegistryValueData) {
        match &self.rpc {
            Some(rpc) => {
                if let Err(e) = rpc.set_value(&path, &value_name, data).await {
                    self.last_error = Some(e.to_string());
                    return;
                }
            }
            None => {
                self.last_error = Some("no peer registry is wired".into());
                return;
            }
        }
        self.request_keys(path).await;
    }

    pub async fn create_key(&mut self, parent_path: String, key_name: String) {
        match &self.rpc {
            Some(rpc) => {
                if let Err(e) = rpc.create_key(&parent_path, &key_name).await {
                    self.last_error = Some(e.to_string());
                    return;
                }
            }
            None => {
                self.last_error = Some("no peer registry is wired".into());
                return;
            }
        }
        self.request_keys(parent_path).await;
    }

    pub async fn delete_entry(&mut self, path: String, value_name: Option<String>, is_key: bool) {
        match &self.rpc {
            Some(rpc) => {
                if let Err(e) = rpc.delete_entry(&path, value_name, is_key).await {
                    self.last_error = Some(e.to_string());
                    return;
                }
            }
            None => {
                self.last_error = Some("no peer registry is wired".into());
                return;
            }
        }
        let refresh = if is_key {
            parent_of(&path).unwrap_or(path)
        } else {
            path
        };
        self.request_keys(refresh).await;
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn state(&self) -> RegistryState {
        RegistryState {
            wired: self.rpc.is_some(),
            last: self.last.clone(),
            last_error: self.last_error.clone(),
        }
    }
}
