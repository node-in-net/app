use common::AppError;
use fm_core::rpc::{FileSystemRpc, RemoteFileEntry};
use nodeinnet_p2p::P2pMessage;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Default)]
pub struct PendingP2p {
    map: Mutex<HashMap<Uuid, oneshot::Sender<P2pMessage>>>,
}

impl PendingP2p {
    pub fn register(&self, id: Uuid) -> oneshot::Receiver<P2pMessage> {
        let (tx, rx) = oneshot::channel();
        self.map.lock().unwrap().insert(id, tx);
        rx
    }

    pub fn forget(&self, id: &Uuid) {
        self.map.lock().unwrap().remove(id);
    }

    pub fn resolve(&self, msg: P2pMessage) -> Option<P2pMessage> {
        let id = match &msg {
            P2pMessage::EntriesResponse { request_id, .. }
            | P2pMessage::MetadataResponse { request_id, .. }
            | P2pMessage::CreateDirectoryResponse { request_id, .. }
            | P2pMessage::DeleteEntryResponse { request_id, .. }
            | P2pMessage::RenameEntryResponse { request_id, .. }
            | P2pMessage::SetPermissionsResponse { request_id, .. }
            | P2pMessage::RegistryKeysResponse { request_id, .. }
            | P2pMessage::SetRegistryValueResponse { request_id, .. }
            | P2pMessage::DeleteRegistryEntryResponse { request_id, .. }
            | P2pMessage::CreateRegistryKeyResponse { request_id, .. } => *request_id,
            P2pMessage::FileTransferResponse { transfer_id, .. } => *transfer_id,
            P2pMessage::AppActionResponse { request_id, .. } => *request_id,
            P2pMessage::AppListResponse {
                request_id: Some(id),
                ..
            } => *id,
            _ => return Some(msg),
        };
        match self.map.lock().unwrap().remove(&id) {
            Some(tx) => {
                let _ = tx.send(msg);
                None
            }
            None => Some(msg),
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.lock().unwrap().len()
    }
}

#[derive(Debug)]
pub enum TransferEvent {
    Progress { bytes_read: u64, total_bytes: u64 },
    Done(Result<(), String>),
}

#[derive(Default)]
pub struct PendingTransfers {
    map: Mutex<HashMap<Uuid, mpsc::UnboundedSender<TransferEvent>>>,
}

impl PendingTransfers {
    pub fn register(&self, id: Uuid) -> mpsc::UnboundedReceiver<TransferEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.map.lock().unwrap().insert(id, tx);
        rx
    }

    pub fn forget(&self, id: &Uuid) {
        self.map.lock().unwrap().remove(id);
    }

    pub fn progress(&self, id: Uuid, bytes_read: u64, total_bytes: u64) {
        if let Some(tx) = self.map.lock().unwrap().get(&id) {
            let _ = tx.send(TransferEvent::Progress {
                bytes_read,
                total_bytes,
            });
        }
    }

    pub fn complete(&self, id: Uuid, result: Result<(), String>) {
        if let Some(tx) = self.map.lock().unwrap().remove(&id) {
            let _ = tx.send(TransferEvent::Done(result));
        }
    }

    pub fn resolve(&self, msg: P2pMessage) -> Option<P2pMessage> {
        let P2pMessage::FileTransferReceipt {
            transfer_id,
            result,
        } = &msg
        else {
            return Some(msg);
        };
        match self.map.lock().unwrap().remove(transfer_id) {
            Some(tx) => {
                let _ = tx.send(TransferEvent::Done(result.clone()));
                None
            }
            None => Some(msg),
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.lock().unwrap().len()
    }
}

async fn confirmation(rx: &mut mpsc::UnboundedReceiver<TransferEvent>) -> Result<(), AppError> {
    loop {
        match rx.recv().await {
            Some(TransferEvent::Done(result)) => return result.map_err(AppError::Other),
            Some(TransferEvent::Progress { .. }) => {}
            None => return Err(AppError::Other("upload dropped".into())),
        }
    }
}

fn ended_early(answer: Result<(), AppError>) -> AppError {
    answer
        .err()
        .unwrap_or_else(|| AppError::Other("the device answered before the upload ended".into()))
}

const UPLOAD_CHUNK: usize = 16 * 1024;

pub struct RemotePeerFsRpc {
    resource_id: String,
    pending: Arc<PendingP2p>,
    transfers: Arc<PendingTransfers>,
    send: Box<dyn Fn(P2pMessage)>,
    bulk: Option<mpsc::Sender<P2pMessage>>,
    timeout: Duration,
    stall_timeout: Duration,
    receipt_timeout: Duration,
}

impl RemotePeerFsRpc {
    pub fn new(
        resource_id: impl Into<String>,
        pending: Arc<PendingP2p>,
        transfers: Arc<PendingTransfers>,
        send: impl Fn(P2pMessage) + 'static,
    ) -> Self {
        Self {
            resource_id: resource_id.into(),
            pending,
            transfers,
            send: Box::new(send),
            bulk: None,
            timeout: Duration::from_secs(15),
            stall_timeout: Duration::from_secs(30),
            receipt_timeout: Duration::from_secs(60 * 60),
        }
    }

    pub fn with_bulk_sender(mut self, tx: mpsc::Sender<P2pMessage>) -> Self {
        self.bulk = Some(tx);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_stall_timeout(mut self, timeout: Duration) -> Self {
        self.stall_timeout = timeout;
        self
    }

    pub fn with_receipt_timeout(mut self, timeout: Duration) -> Self {
        self.receipt_timeout = timeout;
        self
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

    async fn queue(
        &self,
        bulk: &mpsc::Sender<P2pMessage>,
        receipt: &mut mpsc::UnboundedReceiver<TransferEvent>,
        msg: P2pMessage,
    ) -> Result<(), AppError> {
        tokio::select! {
            biased;
            answer = confirmation(receipt) => Err(ended_early(answer)),
            sent = tokio::time::timeout(self.receipt_timeout, bulk.send(msg)) => match sent {
                Ok(Ok(())) => Ok(()),
                Ok(Err(_)) => Err(AppError::Other("the transfer queue closed mid-upload".into())),
                Err(_) => Err(AppError::Other(format!(
                    "the upload stalled: nothing left the queue for {} min",
                    self.receipt_timeout.as_secs() / 60
                ))),
            },
        }
    }
}

fn unexpected(wanted: &str) -> AppError {
    AppError::Other(format!("peer sent an unexpected reply (wanted {wanted})"))
}

impl RemotePeerFsRpc {
    async fn fetch(
        &self,
        path: String,
        offset: u64,
        length: Option<u64>,
        progress_callback: Option<Box<dyn Fn(u64) + 'static>>,
    ) -> Result<Vec<u8>, AppError> {
        let transfer_id = Uuid::new_v4();
        let mut rx = self.transfers.register(transfer_id);
        (self.send)(P2pMessage::FileDownloadRequest {
            resource_id: self.resource_id.clone(),
            file_path: path,
            transfer_id,
            offset,
            length,
        });
        loop {
            match tokio::time::timeout(self.stall_timeout, rx.recv()).await {
                Ok(Some(TransferEvent::Progress { bytes_read, .. })) => {
                    if let Some(cb) = &progress_callback {
                        cb(bytes_read);
                    }
                }
                Ok(Some(TransferEvent::Done(Ok(())))) => {
                    let temp = std::env::temp_dir().join(transfer_id.to_string());
                    let bytes = tokio::fs::read(&temp)
                        .await
                        .map_err(|e| AppError::Other(format!("reading downloaded file: {e}")))?;
                    let _ = tokio::fs::remove_file(&temp).await;
                    return Ok(bytes);
                }
                Ok(Some(TransferEvent::Done(Err(e)))) => return Err(AppError::Other(e)),
                Ok(None) => return Err(AppError::Other("transfer dropped".into())),
                Err(_) => {
                    self.transfers.forget(&transfer_id);
                    return Err(AppError::Other(format!(
                        "the transfer stalled: nothing arrived for {}s",
                        self.stall_timeout.as_secs()
                    )));
                }
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl FileSystemRpc for RemotePeerFsRpc {
    async fn list_dir(&self, path: String) -> Result<Vec<RemoteFileEntry>, AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::RequestEntries {
            request_id,
            resource_id: self.resource_id.clone(),
            path,
        };
        match self.call(request_id, req).await? {
            P2pMessage::EntriesResponse {
                directories,
                files,
                directories_permissions,
                files_permissions,
                ..
            } => {
                let dperm = directories_permissions.unwrap_or_default();
                let fperm = files_permissions.unwrap_or_default();
                let mut out = Vec::with_capacity(directories.len() + files.len());
                for (i, name) in directories.into_iter().enumerate() {
                    out.push(RemoteFileEntry {
                        name,
                        is_dir: true,
                        size: 0,
                        modified: 0,
                        permissions: dperm.get(i).copied().flatten(),
                    });
                }
                for (i, (name, size)) in files.into_iter().enumerate() {
                    out.push(RemoteFileEntry {
                        name,
                        is_dir: false,
                        size,
                        modified: 0,
                        permissions: fperm.get(i).copied().flatten(),
                    });
                }
                Ok(out)
            }
            _ => Err(unexpected("EntriesResponse")),
        }
    }

    async fn create_directory(
        &self,
        parent_path: String,
        dir_name: String,
        permissions: Option<u32>,
    ) -> Result<(), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::CreateDirectoryRequest {
            request_id,
            resource_id: self.resource_id.clone(),
            parent_path,
            dir_name,
            permissions,
        };
        match self.call(request_id, req).await? {
            P2pMessage::CreateDirectoryResponse { result, .. } => result.map_err(AppError::Other),
            _ => Err(unexpected("CreateDirectoryResponse")),
        }
    }

    async fn delete_entries(&self, paths: Vec<String>) -> Result<(), AppError> {
        for path in paths {
            let request_id = Uuid::new_v4();
            let req = P2pMessage::DeleteEntryRequest {
                request_id,
                resource_id: self.resource_id.clone(),
                path,
            };
            match self.call(request_id, req).await? {
                P2pMessage::DeleteEntryResponse { result, .. } => {
                    result.map_err(AppError::Other)?
                }
                _ => return Err(unexpected("DeleteEntryResponse")),
            }
        }
        Ok(())
    }

    async fn rename_entry(&self, path: String, new_path: String) -> Result<(), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::RenameEntryRequest {
            request_id,
            resource_id: self.resource_id.clone(),
            path,
            new_path,
        };
        match self.call(request_id, req).await? {
            P2pMessage::RenameEntryResponse { result, .. } => result.map_err(AppError::Other),
            _ => Err(unexpected("RenameEntryResponse")),
        }
    }

    async fn set_permissions(&self, path: String, permissions: u32) -> Result<(), AppError> {
        let request_id = Uuid::new_v4();
        let req = P2pMessage::SetPermissionsRequest {
            request_id,
            resource_id: self.resource_id.clone(),
            path,
            permissions,
        };
        match self.call(request_id, req).await? {
            P2pMessage::SetPermissionsResponse { result, .. } => result.map_err(AppError::Other),
            _ => Err(unexpected("SetPermissionsResponse")),
        }
    }

    async fn read_file(
        &self,
        path: String,
        progress_callback: Option<Box<dyn Fn(u64) + 'static>>,
    ) -> Result<Vec<u8>, AppError> {
        self.fetch(path, 0, None, progress_callback).await
    }

    async fn read_at(&self, path: String, offset: u64, len: u64) -> Result<Vec<u8>, AppError> {
        if len == 0 {
            return Ok(Vec::new());
        }
        self.fetch(path, offset, Some(len), None).await
    }

    async fn write_file(
        &self,
        path: String,
        content: Vec<u8>,
        permissions: Option<u32>,
        progress_callback: Option<Box<dyn Fn(u64) + 'static>>,
    ) -> Result<(), AppError> {
        let (target_path, file_name) = match path.rfind('/') {
            Some(0) => ("/".to_string(), path[1..].to_string()),
            Some(i) => (path[..i].to_string(), path[i + 1..].to_string()),
            None => ("/".to_string(), path.clone()),
        };
        if file_name.is_empty() {
            return Err(AppError::Other("no file name to write".into()));
        }

        let Some(bulk) = self.bulk.clone() else {
            return Err(AppError::Other(
                "this provider has no queue for bulk transfer; uploads are disabled".into(),
            ));
        };

        let transfer_id = Uuid::new_v4();
        let total_size = content.len() as u64;
        let mut receipt = self.transfers.register(transfer_id);
        let stored: Result<(), AppError> = async {
            let accepted = self.pending.register(transfer_id);
            (self.send)(P2pMessage::FileUploadRequest {
                resource_id: self.resource_id.clone(),
                target_path,
                file_name,
                total_size,
                transfer_id,
                permissions,
            });

            tokio::select! {
                biased;
                answer = confirmation(&mut receipt) => {
                    self.pending.forget(&transfer_id);
                    return Err(ended_early(answer));
                }
                answer = tokio::time::timeout(self.timeout, accepted) => match answer {
                    Ok(Ok(P2pMessage::FileTransferResponse { status, .. })) => match status {
                        nodeinnet_p2p::FileTransferStatus::Accepted { .. } => {}
                        nodeinnet_p2p::FileTransferStatus::Rejected { reason } => {
                            return Err(AppError::Other(format!(
                                "peer refused the upload: {reason}"
                            )))
                        }
                    },
                    Ok(Ok(_)) => return Err(unexpected("FileTransferResponse")),
                    Ok(Err(_)) => return Err(AppError::Other("upload dropped".into())),
                    Err(_) => {
                        self.pending.forget(&transfer_id);
                        return Err(AppError::Other(
                            "peer did not accept the upload in time".into(),
                        ));
                    }
                },
            }

            let mut last_report = std::time::Instant::now() - Duration::from_millis(200);
            let mut offset: usize = 0;
            for chunk in content.chunks(UPLOAD_CHUNK) {
                let msg = P2pMessage::FileChunk {
                    transfer_id,
                    offset: offset as u64,
                    data: chunk.to_vec(),
                };
                self.queue(&bulk, &mut receipt, msg).await?;
                offset += chunk.len();
                if let Some(cb) = &progress_callback {
                    if last_report.elapsed() >= Duration::from_millis(100) {
                        cb(offset as u64);
                        last_report = std::time::Instant::now();
                    }
                }
            }
            if let Some(cb) = &progress_callback {
                cb(offset as u64);
            }

            let complete = P2pMessage::FileTransferComplete {
                transfer_id,
                checksum: None,
            };
            self.queue(&bulk, &mut receipt, complete).await?;
            tokio::time::timeout(self.receipt_timeout, confirmation(&mut receipt))
                .await
                .unwrap_or_else(|_| {
                    Err(AppError::Other(format!(
                        "the device did not confirm the upload within {} min",
                        self.receipt_timeout.as_secs() / 60
                    )))
                })
        }
        .await;
        if stored.is_err() {
            self.transfers.forget(&transfer_id);
        }
        stored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Rig = (
        RemotePeerFsRpc,
        Arc<PendingP2p>,
        Arc<PendingTransfers>,
        Rc<RefCell<Vec<P2pMessage>>>,
    );

    fn rig(answer: impl Fn(&P2pMessage) -> Option<P2pMessage> + 'static) -> Rig {
        let pending = Arc::new(PendingP2p::default());
        let transfers = Arc::new(PendingTransfers::default());
        let sent = Rc::new(RefCell::new(Vec::new()));
        let (p, s) = (pending.clone(), sent.clone());
        let rpc = RemotePeerFsRpc::new("res-1", pending.clone(), transfers.clone(), move |msg| {
            if let Some(resp) = answer(&msg) {
                assert!(p.resolve(resp).is_none(), "reply must be claimed");
            }
            s.borrow_mut().push(msg);
        })
        .with_timeout(Duration::from_millis(200))
        .with_stall_timeout(Duration::from_millis(300));
        (rpc, pending, transfers, sent)
    }

    fn entries_response(req: &P2pMessage) -> Option<P2pMessage> {
        if let P2pMessage::RequestEntries {
            request_id,
            resource_id,
            path,
        } = req
        {
            Some(P2pMessage::EntriesResponse {
                request_id: *request_id,
                resource_id: resource_id.clone(),
                path: path.clone(),
                directories: vec!["docs".into()],
                files: vec![("readme.md".into(), 5)],
                directories_with_dates: None,
                files_with_dates: None,
                directories_permissions: Some(vec![Some(0o755)]),
                files_permissions: Some(vec![None]),
            })
        } else {
            None
        }
    }

    #[tokio::test]
    async fn list_dir_maps_the_wire_response() {
        let (rpc, pending, _transfers, sent) = rig(entries_response);
        let entries = rpc.list_dir("/".into()).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir && entries[0].name == "docs");
        assert_eq!(entries[0].permissions, Some(0o755));
        assert!(!entries[1].is_dir && entries[1].size == 5);
        assert_eq!(pending.len(), 0, "correlation entry consumed");
        assert!(matches!(
            &sent.borrow()[0],
            P2pMessage::RequestEntries { resource_id, .. } if resource_id == "res-1"
        ));
    }

    #[tokio::test]
    async fn peer_error_becomes_app_error() {
        let (rpc, _, _, _) = rig(|req| {
            if let P2pMessage::CreateDirectoryRequest {
                request_id,
                resource_id,
                parent_path,
                ..
            } = req
            {
                Some(P2pMessage::CreateDirectoryResponse {
                    request_id: *request_id,
                    resource_id: resource_id.clone(),
                    parent_path: parent_path.clone(),
                    result: Err("denied".into()),
                })
            } else {
                None
            }
        });
        let err = rpc
            .create_directory("/".into(), "x".into(), None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("denied"));
    }

    #[tokio::test]
    async fn silence_times_out_and_leaks_nothing() {
        let (rpc, pending, _transfers, _) = rig(|_| None);
        let err = rpc.list_dir("/".into()).await.unwrap_err();
        assert!(err.to_string().contains("did not answer"));
        assert_eq!(pending.len(), 0, "timed-out entry must be removed");
    }

    #[tokio::test]
    async fn delete_awaits_every_entry() {
        let count = Rc::new(RefCell::new(0));
        let c = count.clone();
        let (rpc, _, _, _) = rig(move |req| {
            if let P2pMessage::DeleteEntryRequest {
                request_id,
                resource_id,
                ..
            } = req
            {
                *c.borrow_mut() += 1;
                Some(P2pMessage::DeleteEntryResponse {
                    request_id: *request_id,
                    resource_id: resource_id.clone(),
                    parent_path: "/".into(),
                    result: Ok(()),
                })
            } else {
                None
            }
        });
        rpc.delete_entries(vec!["/a".into(), "/b".into()])
            .await
            .unwrap();
        assert_eq!(*count.borrow(), 2);
    }

    #[tokio::test]
    async fn set_permissions_sends_mode_and_awaits_ack() {
        let seen: Rc<RefCell<Option<u32>>> = Rc::new(RefCell::new(None));
        let s = seen.clone();
        let (rpc, _, _, _) = rig(move |req| {
            if let P2pMessage::SetPermissionsRequest {
                request_id,
                resource_id,
                permissions,
                ..
            } = req
            {
                *s.borrow_mut() = Some(*permissions);
                Some(P2pMessage::SetPermissionsResponse {
                    request_id: *request_id,
                    resource_id: resource_id.clone(),
                    parent_path: "/".into(),
                    result: Ok(()),
                })
            } else {
                None
            }
        });
        rpc.set_permissions("/readme.md".into(), 0o640)
            .await
            .unwrap();
        assert_eq!(
            *seen.borrow(),
            Some(0o640),
            "the chosen mode reached the wire"
        );
    }

    #[tokio::test]
    async fn read_file_sends_a_download_request_and_times_out_cleanly() {
        let (rpc, _, transfers, sent) = rig(|_| None);
        let err = rpc.read_file("/readme.md".into(), None).await.unwrap_err();
        assert!(err.to_string().contains("stalled"), "{err}");
        assert!(matches!(
            &sent.borrow()[0],
            P2pMessage::FileDownloadRequest { resource_id, file_path, .. }
                if resource_id == "res-1" && file_path == "/readme.md"
        ));
        let stray = Uuid::new_v4();
        transfers.complete(stray, Ok(()));
    }

    #[tokio::test]
    async fn read_file_returns_the_reassembled_temp_file() {
        let captured: Rc<RefCell<Option<Uuid>>> = Rc::new(RefCell::new(None));
        let cap = captured.clone();
        let pending = Arc::new(PendingP2p::default());
        let transfers = Arc::new(PendingTransfers::default());
        let rpc = RemotePeerFsRpc::new("res-1", pending, transfers.clone(), move |msg| {
            if let P2pMessage::FileDownloadRequest { transfer_id, .. } = msg {
                *cap.borrow_mut() = Some(transfer_id);
            }
        })
        .with_stall_timeout(Duration::from_secs(2));

        let fulfil = async {
            let id = loop {
                if let Some(id) = *captured.borrow() {
                    break id;
                }
                tokio::task::yield_now().await;
            };
            let temp = std::env::temp_dir().join(id.to_string());
            tokio::fs::write(&temp, b"downloaded bytes").await.unwrap();
            transfers.complete(id, Ok(()));
        };
        let (bytes, _) = tokio::join!(rpc.read_file("/readme.md".into(), None), fulfil);
        assert_eq!(bytes.unwrap(), b"downloaded bytes");
    }

    #[tokio::test]
    async fn a_moving_transfer_outlives_the_stall_deadline() {
        let captured: Rc<RefCell<Option<Uuid>>> = Rc::new(RefCell::new(None));
        let cap = captured.clone();
        let pending = Arc::new(PendingP2p::default());
        let transfers = Arc::new(PendingTransfers::default());
        let rpc = RemotePeerFsRpc::new("res-1", pending, transfers.clone(), move |msg| {
            if let P2pMessage::FileDownloadRequest { transfer_id, .. } = msg {
                *cap.borrow_mut() = Some(transfer_id);
            }
        })
        .with_stall_timeout(Duration::from_millis(100));

        let fulfil = async {
            let id = loop {
                if let Some(id) = *captured.borrow() {
                    break id;
                }
                tokio::task::yield_now().await;
            };
            for step in 1..=12u64 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                transfers.progress(id, step * 1024, 12 * 1024);
            }
            let temp = std::env::temp_dir().join(id.to_string());
            tokio::fs::write(&temp, b"slow but alive").await.unwrap();
            transfers.complete(id, Ok(()));
        };
        let seen = Rc::new(RefCell::new(0u64));
        let s = seen.clone();
        let (bytes, _) = tokio::join!(
            rpc.read_file(
                "/big.bin".into(),
                Some(Box::new(move |n| *s.borrow_mut() = n)),
            ),
            fulfil
        );
        assert_eq!(bytes.unwrap(), b"slow but alive");
        assert_eq!(*seen.borrow(), 12 * 1024, "progress must reach the caller");
    }

    #[test]
    fn unclaimed_messages_are_handed_back() {
        let pending = PendingP2p::default();
        assert!(pending.resolve(P2pMessage::Goodbye).is_some());
        let stray = P2pMessage::DeleteEntryResponse {
            request_id: Uuid::new_v4(),
            resource_id: "r".into(),
            parent_path: "/".into(),
            result: Ok(()),
        };
        assert!(pending.resolve(stray).is_some());
    }

    fn accepts_upload(req: &P2pMessage) -> Option<P2pMessage> {
        if let P2pMessage::FileUploadRequest {
            transfer_id,
            total_size,
            ..
        } = req
        {
            Some(P2pMessage::FileTransferResponse {
                transfer_id: *transfer_id,
                status: nodeinnet_p2p::FileTransferStatus::Accepted {
                    total_size: *total_size,
                },
            })
        } else {
            None
        }
    }

    async fn drain_bulk(rx: &mut mpsc::Receiver<P2pMessage>) -> Vec<P2pMessage> {
        let mut out = Vec::new();
        while let Some(msg) = rx.recv().await {
            let done = matches!(msg, P2pMessage::FileTransferComplete { .. });
            out.push(msg);
            if done {
                break;
            }
        }
        out
    }

    fn receipt_of(transfer_id: Uuid, result: Result<(), String>) -> P2pMessage {
        P2pMessage::FileTransferReceipt {
            transfer_id,
            result,
        }
    }

    fn ended(bulk: &[P2pMessage]) -> Uuid {
        match bulk.last() {
            Some(P2pMessage::FileTransferComplete { transfer_id, .. }) => *transfer_id,
            other => panic!("the upload did not end with FileTransferComplete: {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_file_announces_then_streams_chunks() {
        let (rpc, _pending, transfers, sent) = rig(accepts_upload);
        let (tx, mut rx) = mpsc::channel::<P2pMessage>(1);
        let rpc = rpc.with_bulk_sender(tx);
        let content = vec![7u8; UPLOAD_CHUNK * 2 + 5];
        let peer = async {
            let bulk = drain_bulk(&mut rx).await;
            assert!(transfers
                .resolve(receipt_of(ended(&bulk), Ok(())))
                .is_none());
            bulk
        };
        let (res, bulk) = tokio::join!(
            rpc.write_file("/docs/big.bin".into(), content.clone(), Some(0o644), None),
            peer
        );
        res.expect("upload accepted");

        let sent = sent.borrow();
        match &sent[0] {
            P2pMessage::FileUploadRequest {
                target_path,
                file_name,
                total_size,
                permissions,
                ..
            } => {
                assert_eq!(target_path, "/docs");
                assert_eq!(file_name, "big.bin");
                assert_eq!(*total_size, content.len() as u64);
                assert_eq!(*permissions, Some(0o644));
            }
            other => panic!("first message must announce the upload, got {other:?}"),
        }

        let chunks: Vec<(u64, usize)> = bulk
            .iter()
            .filter_map(|m| match m {
                P2pMessage::FileChunk { offset, data, .. } => Some((*offset, data.len())),
                _ => None,
            })
            .collect();
        assert_eq!(
            chunks.len(),
            3,
            "two full chunks and the remainder, none lost"
        );
        assert_eq!(chunks[0], (0, UPLOAD_CHUNK));
        assert_eq!(chunks[1], (UPLOAD_CHUNK as u64, UPLOAD_CHUNK));
        assert_eq!(chunks[2], ((UPLOAD_CHUNK * 2) as u64, 5));
        assert!(
            matches!(bulk.last(), Some(P2pMessage::FileTransferComplete { .. })),
            "the peer is told when to stop waiting",
        );
    }

    async fn upload_answered_with(result: Result<(), String>) -> Result<(), AppError> {
        let (rpc, _pending, transfers, _sent) = rig(accepts_upload);
        let (tx, mut rx) = mpsc::channel::<P2pMessage>(4);
        let rpc = rpc.with_bulk_sender(tx);
        let write = rpc.write_file("/docs/a.txt".into(), b"hello".to_vec(), None, None);
        tokio::pin!(write);
        let bulk = tokio::select! {
            done = &mut write => panic!("write_file returned before the upload ended: {done:?}"),
            bulk = drain_bulk(&mut rx) => bulk,
        };
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut write)
                .await
                .is_err(),
            "write_file returned before the peer answered"
        );
        assert!(transfers
            .resolve(receipt_of(ended(&bulk), result))
            .is_none());
        let written = write.await;
        assert_eq!(transfers.len(), 0);
        written
    }

    #[tokio::test]
    async fn write_file_succeeds_only_once_the_peer_confirms_it() {
        assert_eq!(upload_answered_with(Ok(())).await, Ok(()));
    }

    #[tokio::test]
    async fn write_file_reports_why_the_peer_did_not_store_the_file() {
        let err = upload_answered_with(Err("No space left on device (os error 28)".into()))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("No space left"), "{err}");
    }

    #[tokio::test]
    async fn an_error_while_streaming_stops_the_upload() {
        let (rpc, _pending, transfers, _sent) = rig(accepts_upload);
        let (tx, mut rx) = mpsc::channel::<P2pMessage>(1);
        let rpc = rpc.with_bulk_sender(tx);
        let peer = async {
            let Some(P2pMessage::FileChunk { transfer_id, .. }) = rx.recv().await else {
                panic!("the upload sent no chunk");
            };
            let gone = "the connection to the device ended before it confirmed the upload";
            assert!(transfers
                .resolve(receipt_of(transfer_id, Err(gone.into())))
                .is_none());
            let mut more = Vec::new();
            while let Ok(Some(msg)) =
                tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
            {
                more.push(msg);
            }
            more
        };
        let (written, more) = tokio::join!(
            rpc.write_file("/big.bin".into(), vec![1; UPLOAD_CHUNK * 10], None, None),
            peer
        );

        let err = written.unwrap_err();
        assert!(
            err.to_string().contains("ended before it confirmed"),
            "{err}"
        );
        assert!(more.len() <= 1, "{} more chunks went out", more.len());
        assert!(!more
            .iter()
            .any(|m| matches!(m, P2pMessage::FileTransferComplete { .. })));
        assert_eq!(transfers.len(), 0);
    }

    #[tokio::test]
    async fn a_receipt_in_place_of_acceptance_fails_the_upload() {
        let captured: Rc<RefCell<Option<Uuid>>> = Rc::new(RefCell::new(None));
        let (rpc, pending, transfers, _sent) = rig({
            let captured = captured.clone();
            move |req| {
                if let P2pMessage::FileUploadRequest { transfer_id, .. } = req {
                    *captured.borrow_mut() = Some(*transfer_id);
                }
                None
            }
        });
        let (tx, _rx) = mpsc::channel::<P2pMessage>(4);
        let rpc = rpc
            .with_bulk_sender(tx)
            .with_timeout(Duration::from_secs(5));
        let peer = async {
            let transfer_id = loop {
                if let Some(id) = *captured.borrow() {
                    break id;
                }
                tokio::task::yield_now().await;
            };
            let gone = "the connection to the device ended before it confirmed the upload";
            assert!(transfers
                .resolve(receipt_of(transfer_id, Err(gone.into())))
                .is_none());
        };
        let started = std::time::Instant::now();
        let (written, _) = tokio::join!(
            rpc.write_file("/x.bin".into(), vec![1, 2, 3], None, None),
            peer
        );

        assert!(written.unwrap_err().to_string().contains("ended before"));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!((pending.len(), transfers.len()), (0, 0));
    }

    #[tokio::test]
    async fn write_file_gives_up_when_no_receipt_comes() {
        let (rpc, _pending, transfers, _sent) = rig(accepts_upload);
        let (tx, mut rx) = mpsc::channel::<P2pMessage>(4);
        let rpc = rpc
            .with_bulk_sender(tx)
            .with_receipt_timeout(Duration::from_millis(200));
        let (written, _) = tokio::join!(
            rpc.write_file("/x.bin".into(), vec![1, 2, 3], None, None),
            drain_bulk(&mut rx)
        );

        assert!(
            written.unwrap_err().to_string().contains("did not confirm"),
            "the upload passed without a receipt"
        );
        assert_eq!(transfers.len(), 0);
    }

    #[test]
    fn a_receipt_nobody_waits_for_is_handed_back() {
        let transfers = PendingTransfers::default();
        let stray = receipt_of(Uuid::new_v4(), Ok(()));
        assert!(transfers.resolve(stray).is_some());
        assert!(transfers.resolve(P2pMessage::Goodbye).is_some());
    }

    #[tokio::test]
    async fn write_file_without_a_bulk_queue_refuses_rather_than_corrupts() {
        let (rpc, _pending, _transfers, sent) = rig(accepts_upload);
        let err = rpc
            .write_file("/x.bin".into(), vec![1, 2, 3], None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("uploads are disabled"), "{err}");
        assert!(sent.borrow().is_empty(), "nothing may be announced either");
    }

    #[tokio::test]
    async fn write_file_reports_the_peers_refusal() {
        let (rpc, _pending, _transfers, sent) = rig(|req| {
            if let P2pMessage::FileUploadRequest { transfer_id, .. } = req {
                Some(P2pMessage::FileTransferResponse {
                    transfer_id: *transfer_id,
                    status: nodeinnet_p2p::FileTransferStatus::Rejected {
                        reason: "read-only share".into(),
                    },
                })
            } else {
                None
            }
        });
        let (tx, _rx) = mpsc::channel::<P2pMessage>(8);
        let err = rpc
            .with_bulk_sender(tx)
            .write_file("/x.bin".into(), vec![1, 2, 3], None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("read-only share"), "{err}");
        assert!(
            !sent
                .borrow()
                .iter()
                .any(|m| matches!(m, P2pMessage::FileChunk { .. })),
            "a refused upload must not put bytes on the wire",
        );
    }

    #[tokio::test]
    async fn write_file_times_out_when_the_peer_never_answers() {
        let (rpc, _pending, _transfers, sent) = rig(|_| None);
        let (tx, _rx) = mpsc::channel::<P2pMessage>(8);
        let err = rpc
            .with_bulk_sender(tx)
            .write_file("/x.bin".into(), vec![1, 2, 3], None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("did not accept"), "{err}");
        assert!(
            !sent
                .borrow()
                .iter()
                .any(|m| matches!(m, P2pMessage::FileChunk { .. })),
            "no bytes before the handshake completes",
        );
    }
}

#[cfg(test)]
mod remote_launch_correlation {
    use super::PendingP2p;
    use nodeinnet_p2p::P2pMessage;
    use uuid::Uuid;

    #[test]
    fn a_launch_reply_reaches_the_caller_that_is_waiting() {
        let pending = PendingP2p::default();
        let id = Uuid::new_v4();
        let mut rx = pending.register(id);

        let claimed = pending.resolve(P2pMessage::AppActionResponse {
            resource_id: "network-abc".into(),
            request_id: id,
            session_id: None,
            error: None,
            detail: None,
        });

        assert!(
            claimed.is_none(),
            "the reply must be claimed, not passed on"
        );
        assert!(rx.try_recv().is_ok(), "the waiting caller was woken");
    }

    #[test]
    fn a_list_reply_reaches_the_caller_that_is_waiting() {
        let pending = PendingP2p::default();
        let id = Uuid::new_v4();
        let mut rx = pending.register(id);

        let claimed = pending.resolve(P2pMessage::AppListResponse {
            resource_id: "network-abc".into(),
            request_id: Some(id),
            apps: Vec::new(),
            sessions: Vec::new(),
            refused: None,
            event: None,
        });

        assert!(claimed.is_none());
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn an_unsolicited_push_is_passed_on() {
        let pending = PendingP2p::default();
        let push = P2pMessage::AppListResponse {
            resource_id: "network-abc".into(),
            request_id: None,
            apps: Vec::new(),
            sessions: Vec::new(),
            refused: None,
            event: Some("exited".into()),
        };
        assert!(
            pending.resolve(push).is_some(),
            "a push has no caller to wake and must reach the router"
        );
    }
}
