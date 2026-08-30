use nodeinnet_p2p::P2pMessage;
use std::sync::Mutex;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

lazy_static::lazy_static! {
    static ref LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

#[wasm_bindgen]
pub fn init_node() -> String {
    console_error_panic_hook::set_once();
    "WASM Node Initialized successfully".to_string()
}

#[wasm_bindgen]
pub fn process_message_json(json_str: &str) -> String {
    let env_err = match serde_json::from_str::<nodeinnet_p2p::SecuredP2pEnvelope>(json_str) {
        Ok(envelope) => {
            return serde_json::to_string(&envelope.message)
                .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
        }
        Err(e) => e,
    };

    match serde_json::from_str::<P2pMessage>(json_str) {
        Ok(msg) => serde_json::to_string(&msg).unwrap_or_else(|e| format!("{}", e)),
        Err(fallback_err) => {
            format!(
                "{{\"error\": \"EnvErr: {}; FallbackErr: {}\"}}",
                env_err, fallback_err
            )
        }
    }
}

fn wrap_envelope(msg: P2pMessage) -> String {
    let envelope = nodeinnet_p2p::SecuredP2pEnvelope {
        mac: None,
        message: msg,
    };
    serde_json::to_string(&envelope).unwrap_or_default()
}

#[wasm_bindgen]
pub fn generate_sysinfo_request(resource_id: &str) -> String {
    let msg = P2pMessage::RequestSystemInfo {
        resource_id: resource_id.to_string(),
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_terminal_start(resource_id: &str) -> String {
    let msg = P2pMessage::StartTerminal {
        resource_id: resource_id.to_string(),
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_terminal_stop(resource_id: &str) -> String {
    let msg = P2pMessage::StopTerminal {
        resource_id: resource_id.to_string(),
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_terminal_input(resource_id: &str, data: &[u8]) -> String {
    let msg = P2pMessage::TerminalInput {
        resource_id: resource_id.to_string(),
        data: data.to_vec(),
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_terminal_resize(resource_id: &str, rows: u16, cols: u16) -> String {
    let msg = P2pMessage::TerminalResize {
        resource_id: resource_id.to_string(),
        rows,
        cols,
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_http_request(
    resource_id: &str,
    request_id: &str,
    method: &str,
    url: &str,
    headers_json: &str,
    body: Option<Vec<u8>>,
) -> String {
    let req_id = Uuid::parse_str(request_id).unwrap_or_else(|_| Uuid::new_v4());

    let headers: std::collections::BTreeMap<String, String> =
        serde_json::from_str(headers_json).unwrap_or_default();

    let msg = P2pMessage::HttpRequest {
        resource_id: resource_id.to_string(),
        request_id: req_id,
        method: method.to_string(),
        url: url.to_string(),
        headers,
        body,
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_entries_request(resource_id: &str, path: &str) -> String {
    let msg = P2pMessage::RequestEntries {
        request_id: Uuid::new_v4(),
        resource_id: resource_id.to_string(),
        path: path.to_string(),
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_create_directory(resource_id: &str, parent_path: &str, dir_name: &str) -> String {
    let msg = P2pMessage::CreateDirectoryRequest {
        request_id: Uuid::new_v4(),
        resource_id: resource_id.to_string(),
        parent_path: parent_path.to_string(),
        dir_name: dir_name.to_string(),
        permissions: None,
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_delete_entry(resource_id: &str, path: &str) -> String {
    let msg = P2pMessage::DeleteEntryRequest {
        request_id: Uuid::new_v4(),
        resource_id: resource_id.to_string(),
        path: path.to_string(),
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_rename_entry(resource_id: &str, path: &str, new_path: &str) -> String {
    let msg = P2pMessage::RenameEntryRequest {
        request_id: Uuid::new_v4(),
        resource_id: resource_id.to_string(),
        path: path.to_string(),
        new_path: new_path.to_string(),
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_file_download_request(resource_id: &str, path: &str, transfer_id: &str) -> String {
    if let Ok(tid) = Uuid::parse_str(transfer_id) {
        let msg = P2pMessage::FileDownloadRequest {
            transfer_id: tid,
            resource_id: resource_id.to_string(),
            file_path: path.to_string(),
        };
        wrap_envelope(msg)
    } else {
        "".to_string()
    }
}

#[wasm_bindgen]
pub fn generate_file_upload_request(
    resource_id: &str,
    path: &str,
    file_name: &str,
    file_size: f64,
    transfer_id: &str,
) -> String {
    if let Ok(tid) = Uuid::parse_str(transfer_id) {
        let msg = P2pMessage::FileUploadRequest {
            transfer_id: tid,
            resource_id: resource_id.to_string(),
            target_path: path.to_string(),
            file_name: file_name.to_string(),
            total_size: file_size as u64,
            permissions: None,
        };
        wrap_envelope(msg)
    } else {
        "".to_string()
    }
}

#[wasm_bindgen]
pub fn generate_file_chunk(transfer_id: &str, offset: f64, data: &[u8]) -> String {
    if let Ok(tid) = Uuid::parse_str(transfer_id) {
        let msg = P2pMessage::FileChunk {
            transfer_id: tid,
            offset: offset as u64,
            data: data.to_vec(),
        };
        wrap_envelope(msg)
    } else {
        "".to_string()
    }
}

#[wasm_bindgen]
pub fn generate_file_transfer_complete(transfer_id: &str) -> String {
    if let Ok(tid) = Uuid::parse_str(transfer_id) {
        let msg = P2pMessage::FileTransferComplete {
            transfer_id: tid,
            checksum: None,
        };
        wrap_envelope(msg)
    } else {
        "".to_string()
    }
}

#[wasm_bindgen]
pub fn generate_keypair() -> js_sys::Array {
    let (priv_b64, pub_b64) = nodeinnet_p2p::crypto::generate_ed25519_keypair();
    let arr = js_sys::Array::new();
    arr.push(&js_sys::JsString::from(priv_b64));
    arr.push(&js_sys::JsString::from(pub_b64));
    arr
}

#[wasm_bindgen]
pub fn sign_p2p_envelope(envelope_json: &str, token: &str) -> Vec<u8> {
    if let Ok(mut envelope) =
        serde_json::from_str::<nodeinnet_p2p::SecuredP2pEnvelope>(envelope_json)
    {
        if let Ok(bson_bytes) = nodeinnet_p2p::p2p::to_bson_vec(&envelope.message) {
            envelope.mac = Some(nodeinnet_p2p::crypto::compute_hmac_sha256(
                &bson_bytes,
                token,
            ));
            if let Ok(full_bson) = nodeinnet_p2p::p2p::to_bson_vec(&envelope) {
                return full_bson;
            }
        }
    }
    envelope_json.as_bytes().to_vec()
}

#[wasm_bindgen]
pub fn generate_signed_handshake(
    my_id: &str,
    peer_id: &str,
    private_key_b64: &str,
    timestamp_ms: f64,
) -> Vec<u8> {
    let ts = timestamp_ms as u64;
    let signature = nodeinnet_p2p::crypto::sign_p2p_handshake(private_key_b64, my_id, peer_id, ts)
        .unwrap_or_else(|_| "INVALID_SIG".to_string());

    let handshake = P2pMessage::Handshake {
        node_version: app_version::APP_VERSION.to_string(),
        timestamp_ms: ts,
        signature,
        requested_resources: None,
        max_chunk_size: Some(10240),
        local_tcp_port: None,
        from_node_id: Some(my_id.to_string()),
    };
    let envelope = nodeinnet_p2p::SecuredP2pEnvelope {
        mac: None,
        message: handshake,
    };
    nodeinnet_p2p::p2p::to_bson_vec(&envelope).unwrap_or_default()
}

#[wasm_bindgen]
pub fn generate_registry_keys_request(resource_id: &str, path: &str) -> String {
    let msg = P2pMessage::RequestRegistryKeys {
        request_id: Uuid::new_v4(),
        resource_id: resource_id.to_string(),
        path: path.to_string(),
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_set_registry_value_request(
    resource_id: &str,
    path: &str,
    value_name: &str,
    value_data_json: &str,
) -> String {
    let value_data: nodeinnet_p2p::p2p::RegistryValueData = serde_json::from_str(value_data_json)
        .unwrap_or(nodeinnet_p2p::p2p::RegistryValueData::Unknown);

    let msg = P2pMessage::SetRegistryValueRequest {
        request_id: Uuid::new_v4(),
        resource_id: resource_id.to_string(),
        path: path.to_string(),
        value_name: value_name.to_string(),
        value_data,
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn get_nodeinnet_version() -> String {
    app_version::APP_VERSION.to_string()
}

#[wasm_bindgen]
pub fn generate_remote_desktop_request(
    resource_id: &str,
    start: bool,
    original_size: bool,
    bitrate_bps: Option<u32>,
) -> String {
    let msg = P2pMessage::RemoteDesktopRequest {
        resource_id: resource_id.to_string(),
        start,
        original_size,
        bitrate_bps,
        force_select: false,
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_remote_desktop_input_mouse_move(resource_id: &str, x: i32, y: i32) -> String {
    let msg = P2pMessage::RemoteDesktopInput {
        resource_id: resource_id.to_string(),
        event: nodeinnet_p2p::p2p::DesktopInputEvent::MouseMove { x, y },
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_remote_desktop_input_mouse_down(resource_id: &str, button: u8) -> String {
    let msg = P2pMessage::RemoteDesktopInput {
        resource_id: resource_id.to_string(),
        event: nodeinnet_p2p::p2p::DesktopInputEvent::MouseDown { button },
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_remote_desktop_input_mouse_up(resource_id: &str, button: u8) -> String {
    let msg = P2pMessage::RemoteDesktopInput {
        resource_id: resource_id.to_string(),
        event: nodeinnet_p2p::p2p::DesktopInputEvent::MouseUp { button },
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_remote_desktop_input_mouse_scroll(resource_id: &str, dy: i32) -> String {
    let msg = P2pMessage::RemoteDesktopInput {
        resource_id: resource_id.to_string(),
        event: nodeinnet_p2p::p2p::DesktopInputEvent::MouseScroll { dy },
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_remote_desktop_input_key_press(resource_id: &str, keyval: u32) -> String {
    let msg = P2pMessage::RemoteDesktopInput {
        resource_id: resource_id.to_string(),
        event: nodeinnet_p2p::p2p::DesktopInputEvent::KeyPress { keyval },
    };
    wrap_envelope(msg)
}

#[wasm_bindgen]
pub fn generate_remote_desktop_input_key_release(resource_id: &str, keyval: u32) -> String {
    let msg = P2pMessage::RemoteDesktopInput {
        resource_id: resource_id.to_string(),
        event: nodeinnet_p2p::p2p::DesktopInputEvent::KeyRelease { keyval },
    };
    wrap_envelope(msg)
}
