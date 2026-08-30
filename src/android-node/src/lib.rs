use async_trait::async_trait;
use client_core::{AppEventHandler, NetCmd};
use jni::objects::{GlobalRef, JClass, JString, JValue};
use jni::sys::jstring;
use jni::{JNIEnv, JavaVM};
use nodeinnet_p2p::NodeInfo;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::mpsc as tokio_mpsc;

static JVM: OnceLock<JavaVM> = OnceLock::new();

lazy_static::lazy_static! {
    static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");
    static ref NET_TX: Mutex<Option<tokio_mpsc::Sender<crate::NetCmd>>> = Mutex::new(None);
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_testNativeLoad<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    let output = env
        .new_string("NodeInNet Native Core Successfully Loaded!")
        .expect("Couldn't create java string!");

    output.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_generateKeyPair<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    let (priv_key, pub_key) = nodeinnet_p2p::crypto::generate_ed25519_keypair();
    let json = serde_json::json!({
        "priv": priv_key,
        "pub": pub_key
    })
    .to_string();

    let output = env.new_string(json).unwrap();
    output.into_raw()
}

#[derive(serde::Serialize)]
struct ResourceWrapper {
    display_name: Option<String>,
    os: Option<String>,
    app_type: Option<String>,
    version: Option<String>,
    resources: Vec<nodeinnet_p2p::p2p::SharedResource>,
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_encodeResourcesToBsonBase64<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    node_info_json: JString<'local>,
) -> jstring {
    let json_str: String = env
        .get_string(&node_info_json)
        .expect("Couldn't get java string!")
        .into();

    let mut b64_out = String::new();
    if let Ok(info) = serde_json::from_str::<NodeInfo>(&json_str) {
        let wrapper = ResourceWrapper {
            display_name: Some(info.name.clone()),
            os: Some(info.os.clone()),
            app_type: Some(info.app_type.clone()),
            version: Some(info.version.clone()),
            resources: info.resources.iter().map(|r| r.without_config()).collect(),
        };
        if let Ok(bson_bytes) = bson::ser::serialize_to_vec(&wrapper) {
            use base64::{engine::general_purpose, Engine as _};
            b64_out = general_purpose::STANDARD.encode(&bson_bytes);
        }
    }

    let output = env.new_string(b64_out).unwrap();
    output.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_connectNode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    node_info_json: JString<'local>,
    ws_url_jstr: JString<'local>,
    private_key_b64_jstr: JString<'local>,
    turn_credentials_json_jstr: JString<'local>,
    callback_instance: jni::objects::JObject<'local>,
) {
    if JVM.get().is_none() {
        if let Ok(jvm) = env.get_java_vm() {
            let _ = JVM.set(jvm);
        }
    }

    let node_info_str: String = env.get_string(&node_info_json).unwrap().into();
    let ws_url: String = env.get_string(&ws_url_jstr).unwrap().into();
    let private_key_b64: String = env.get_string(&private_key_b64_jstr).unwrap().into();

    let mut turn_creds: Option<nodeinnet_p2p::rtc::TurnCredentials> = None;
    if !turn_credentials_json_jstr.is_null() {
        if let Ok(turn_str) = env.get_string(&turn_credentials_json_jstr) {
            let s: String = turn_str.into();
            if !s.is_empty() {
                turn_creds = serde_json::from_str(&s).ok();
            }
        }
    }

    let my_info: nodeinnet_p2p::NodeInfo = match serde_json::from_str(&node_info_str) {
        Ok(info) => info,
        Err(_e) => {
            return;
        }
    };

    let global_callback = match env.new_global_ref(callback_instance) {
        Ok(gref) => Arc::new(gref),
        Err(_) => return,
    };

    let (net_tx, net_rx) = tokio_mpsc::channel(32);

    if let Ok(mut tx_guard) = NET_TX.lock() {
        *tx_guard = Some(net_tx.clone());
    }

    let net_tx_clone = net_tx.clone();
    let my_info_clone = my_info.clone();
    RUNTIME.spawn(async move {
        let _ = net_tx_clone
            .send(NetCmd::Connect(ws_url, my_info_clone, turn_creds))
            .await;
    });

    let handler = Arc::new(AndroidEventHandler { global_callback });
    let config = client_config::AppConfig::new("nodeinnet");

    p2p_node::set_app_version(app_version::APP_VERSION);
    p2p_handlers::install(
        p2p_handlers::Capabilities::FILESYSTEM
            | p2p_handlers::Capabilities::NETWORK
            | p2p_handlers::Capabilities::SYSTEM_INFO,
        p2p_handlers::HostSettings::default(),
    );

    client_core::network::start_network_thread(
        net_rx,
        net_tx,
        handler,
        my_info,
        private_key_b64,
        std::sync::Arc::new(client_config::ConfigPeerStore::new(config.clone())),
        config
            .get::<bool>(client_config::LOCAL_DISCOVERY_KEY)
            .unwrap_or(false),
    );
}

fn invoke_kotlin_callback(gref: &Arc<GlobalRef>, method_name: &str, payload: &str) {
    if let Some(jvm) = JVM.get() {
        if let Ok(mut env) = jvm.attach_current_thread_permanently() {
            let _ = env.with_local_frame::<_, _, jni::errors::Error>(16, |local_env| {
                let jstr = local_env.new_string(payload)?;
                let _ = local_env.call_method(
                    gref.as_obj(),
                    method_name,
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&jstr)],
                );
                Ok(())
            });
        }
    }
}

fn transfer_callback(
    gref: &Arc<GlobalRef>,
    method_name: &str,
    signature: &str,
    transfer_id: &str,
    rest: &[JValue],
) {
    if let Some(jvm) = JVM.get() {
        if let Ok(mut env) = jvm.attach_current_thread_permanently() {
            let _ = env.with_local_frame::<_, _, jni::errors::Error>(16, |local_env| {
                let jstr = local_env.new_string(transfer_id)?;
                let mut args = vec![JValue::Object(&jstr)];
                args.extend_from_slice(rest);
                let _ = local_env.call_method(gref.as_obj(), method_name, signature, &args);
                Ok(())
            });
        }
    }
}

fn invoke_kotlin_callback_empty(gref: &Arc<GlobalRef>, method_name: &str) {
    if let Some(jvm) = JVM.get() {
        if let Ok(mut env) = jvm.attach_current_thread_permanently() {
            let _ = env.call_method(gref.as_obj(), method_name, "()V", &[]);
        }
    }
}

struct AndroidEventHandler {
    global_callback: Arc<GlobalRef>,
}

#[async_trait]
impl AppEventHandler for AndroidEventHandler {
    async fn on_log(&self, msg: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S.%3f").to_string();
        let formatted_msg = format!("[{}] {}", timestamp, msg);
        invoke_kotlin_callback(&self.global_callback, "onLog", &formatted_msg);
    }
    async fn on_connected(&self) {
        invoke_kotlin_callback_empty(&self.global_callback, "onConnected");
    }
    async fn on_disconnected(&self) {
        invoke_kotlin_callback_empty(&self.global_callback, "onDisconnected");
    }
    async fn on_update_nodes(&self, nodes: Vec<NodeInfo>) {
        if let Ok(json) = serde_json::to_string(&nodes) {
            invoke_kotlin_callback(&self.global_callback, "onUpdateNodes", &json);
        }
    }
    async fn on_download_complete(&self, _p: std::path::PathBuf) {}

    async fn on_p2p_message(&self, msg: nodeinnet_p2p::P2pMessage) {
        if let Ok(json) = serde_json::to_string(&msg) {
            invoke_kotlin_callback(&self.global_callback, "onP2pMessage", &json);
        }
    }
    async fn on_p2p_connected(&self, peer_id: String) {
        invoke_kotlin_callback(&self.global_callback, "onP2pConnected", &peer_id);
    }
    async fn on_p2p_disconnected(&self, peer_id: String) {
        invoke_kotlin_callback(&self.global_callback, "onP2pDisconnected", &peer_id);
    }

    async fn on_peer_state_changed(&self, peer_id: String, state: client_core::P2pPeerState) {
        let json = serde_json::json!({ "peer": peer_id, "state": format!("{:?}", state) });
        invoke_kotlin_callback(&self.global_callback, "onPeerState", &json.to_string());
    }

    async fn on_local_p2p_event(&self, event: p2p_node::LocalP2pEvent) {
        match event {
            p2p_node::LocalP2pEvent::RemoteDesktopFrame {
                resource_id,
                bgra_data,
                width,
                height,
                ..
            } => {
                if let Some(jvm) = JVM.get() {
                    if let Ok(mut env) = jvm.attach_current_thread_permanently() {
                        let _ = env.with_local_frame::<_, _, jni::errors::Error>(16, |local_env| {
                            let jstr = local_env.new_string(&resource_id)?;
                            let jarray = local_env.byte_array_from_slice(&bgra_data)?;
                            let _ = local_env.call_method(
                                self.global_callback.as_obj(),
                                "onRemoteDesktopFrame",
                                "(Ljava/lang/String;[BII)V",
                                &[
                                    JValue::Object(&jstr),
                                    JValue::Object(&jarray),
                                    JValue::Int(width as i32),
                                    JValue::Int(height as i32),
                                ],
                            );
                            Ok(())
                        });
                    }
                }
            }
            p2p_node::LocalP2pEvent::RemoteDesktopStopped { resource_id } => {
                invoke_kotlin_callback(
                    &self.global_callback,
                    "onRemoteDesktopStopped",
                    &resource_id,
                );
            }
            p2p_node::LocalP2pEvent::TransferProgress {
                transfer_id,
                bytes_read,
                total_bytes,
                ..
            } => {
                transfer_callback(
                    &self.global_callback,
                    "onTransferProgress",
                    "(Ljava/lang/String;JJ)V",
                    &transfer_id.to_string(),
                    &[
                        JValue::Long(bytes_read as i64),
                        JValue::Long(total_bytes as i64),
                    ],
                );
            }
            p2p_node::LocalP2pEvent::TransferComplete {
                transfer_id,
                is_upload,
                ..
            } => {
                transfer_callback(
                    &self.global_callback,
                    "onTransferComplete",
                    "(Ljava/lang/String;Z)V",
                    &transfer_id.to_string(),
                    &[JValue::Bool(is_upload as u8)],
                );
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_sendP2pMessage<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    peer_id_jstr: jni::objects::JString<'local>,
    msg_json_jstr: jni::objects::JString<'local>,
) {
    let peer_id: String = match env.get_string(&peer_id_jstr) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let json: String = match env.get_string(&msg_json_jstr) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    if let Ok(p2p_msg) = serde_json::from_str::<nodeinnet_p2p::P2pMessage>(&json) {
        if let Ok(tx_guard) = NET_TX.lock() {
            if let Some(tx) = tx_guard.as_ref() {
                let tx_clone = tx.clone();
                RUNTIME.spawn(async move {
                    let _ = tx_clone
                        .send(crate::NetCmd::SendToPeer(peer_id, p2p_msg))
                        .await;
                });
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_reconnectNode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    node_info_json: JString<'local>,
    ws_url_jstr: JString<'local>,
    turn_credentials_json_jstr: JString<'local>,
) {
    let node_info_str: String = match env.get_string(&node_info_json) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let ws_url: String = match env.get_string(&ws_url_jstr) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    let mut turn_creds: Option<nodeinnet_p2p::rtc::TurnCredentials> = None;
    if !turn_credentials_json_jstr.is_null() {
        if let Ok(turn_str) = env.get_string(&turn_credentials_json_jstr) {
            let s: String = turn_str.into();
            if !s.is_empty() {
                turn_creds = serde_json::from_str(&s).ok();
            }
        }
    }

    let my_info: nodeinnet_p2p::NodeInfo = match serde_json::from_str(&node_info_str) {
        Ok(info) => info,
        Err(_) => return,
    };

    if let Ok(tx_guard) = NET_TX.lock() {
        if let Some(tx) = tx_guard.as_ref() {
            let tx_clone = tx.clone();
            RUNTIME.spawn(async move {
                let _ = tx_clone
                    .send(NetCmd::Connect(ws_url, my_info, turn_creds))
                    .await;
            });
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_applyTurnCredentials<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    turn_credentials_json_jstr: JString<'local>,
) {
    let mut turn_creds: Option<nodeinnet_p2p::rtc::TurnCredentials> = None;
    if !turn_credentials_json_jstr.is_null() {
        if let Ok(turn_str) = env.get_string(&turn_credentials_json_jstr) {
            let s: String = turn_str.into();
            if !s.is_empty() {
                turn_creds = serde_json::from_str(&s).ok();
            }
        }
    }

    if let Ok(tx_guard) = NET_TX.lock() {
        if let Some(tx) = tx_guard.as_ref() {
            let tx_clone = tx.clone();
            RUNTIME.spawn(async move {
                let _ = tx_clone
                    .send(NetCmd::ApplyTurnCredentials(turn_creds))
                    .await;
            });
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_callPeer<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    peer_id_jstr: jni::objects::JString<'local>,
) {
    let peer_id: String = match env.get_string(&peer_id_jstr) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    if let Ok(tx_guard) = NET_TX.lock() {
        if let Some(tx) = tx_guard.as_ref() {
            let tx_clone = tx.clone();
            RUNTIME.spawn(async move {
                let _ = tx_clone.send(crate::NetCmd::Call(peer_id)).await;
            });
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_broadcastP2pMessage<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    target_resource_type_jstr: jni::objects::JString<'local>,
    msg_json_jstr: jni::objects::JString<'local>,
) {
    let target_resource_type: String = match env.get_string(&target_resource_type_jstr) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let json: String = match env.get_string(&msg_json_jstr) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    if let Ok(p2p_msg) = serde_json::from_str::<nodeinnet_p2p::P2pMessage>(&json) {
        let res_type = match target_resource_type.as_str() {
            "Filesystem" => nodeinnet_p2p::p2p::ResourceType::Filesystem,
            "SystemInfo" => nodeinnet_p2p::p2p::ResourceType::SystemInfo,
            "Terminal" => nodeinnet_p2p::p2p::ResourceType::Terminal,
            "Registry" => nodeinnet_p2p::p2p::ResourceType::Registry,
            "SharedNetwork" => nodeinnet_p2p::p2p::ResourceType::SharedNetwork,
            "SyncFolder" => nodeinnet_p2p::p2p::ResourceType::SyncFolder,
            _ => {
                return;
            }
        };

        if let Ok(tx_guard) = NET_TX.lock() {
            if let Some(tx) = tx_guard.as_ref() {
                let tx_clone = tx.clone();
                RUNTIME.spawn(async move {
                    let _ = tx_clone
                        .send(crate::NetCmd::BroadcastP2pMessage {
                            target_resource_type: res_type,
                            msg_template: p2p_msg,
                        })
                        .await;
                });
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_setTempDir<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    dir_jstr: jni::objects::JString<'local>,
) {
    if let Ok(dir) = env.get_string(&dir_jstr) {
        let dir: String = dir.into();
        std::env::set_var("TMPDIR", dir);
    }
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_startDownload<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    peer_id_jstr: jni::objects::JString<'local>,
    resource_id_jstr: jni::objects::JString<'local>,
    remote_path_jstr: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let empty = |env: &mut jni::JNIEnv| {
        env.new_string("")
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    };

    let (peer_id, resource_id, file_path) = match (
        env.get_string(&peer_id_jstr),
        env.get_string(&resource_id_jstr),
        env.get_string(&remote_path_jstr),
    ) {
        (Ok(p), Ok(r), Ok(f)) => (p.into(), r.into(), f.into()),
        _ => return empty(&mut env),
    };

    let transfer_id = uuid::Uuid::new_v4();
    let tx = match NET_TX.lock().ok().and_then(|g| g.clone()) {
        Some(tx) => tx,
        None => return empty(&mut env),
    };
    RUNTIME.spawn(async move {
        let _ = tx
            .send(crate::NetCmd::SendToPeer(
                peer_id,
                nodeinnet_p2p::P2pMessage::FileDownloadRequest {
                    resource_id,
                    file_path,
                    transfer_id,
                },
            ))
            .await;
    });

    env.new_string(transfer_id.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_startUpload<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    peer_id_jstr: jni::objects::JString<'local>,
    resource_id_jstr: jni::objects::JString<'local>,
    local_path_jstr: jni::objects::JString<'local>,
    target_path_jstr: jni::objects::JString<'local>,
    file_name_jstr: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let empty = |env: &mut jni::JNIEnv| {
        env.new_string("")
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    };

    let (peer_id, resource_id, local_path, target_path, file_name): (
        String,
        String,
        String,
        String,
        String,
    ) = match (
        env.get_string(&peer_id_jstr),
        env.get_string(&resource_id_jstr),
        env.get_string(&local_path_jstr),
        env.get_string(&target_path_jstr),
        env.get_string(&file_name_jstr),
    ) {
        (Ok(p), Ok(r), Ok(l), Ok(t), Ok(n)) => (p.into(), r.into(), l.into(), t.into(), n.into()),
        _ => return empty(&mut env),
    };

    let local_file_path = std::path::PathBuf::from(&local_path);
    let total_size = match std::fs::metadata(&local_file_path) {
        Ok(m) => m.len(),
        Err(_) => return empty(&mut env),
    };

    let transfer_id = uuid::Uuid::new_v4();
    let tx = match NET_TX.lock().ok().and_then(|g| g.clone()) {
        Some(tx) => tx,
        None => return empty(&mut env),
    };
    RUNTIME.spawn(async move {
        let _ = tx
            .send(crate::NetCmd::RegisterUpload {
                peer_id: peer_id.clone(),
                transfer_id,
                local_file_path,
            })
            .await;
        let _ = tx
            .send(crate::NetCmd::SendToPeer(
                peer_id,
                nodeinnet_p2p::P2pMessage::FileUploadRequest {
                    resource_id,
                    target_path,
                    file_name,
                    total_size,
                    transfer_id,
                    permissions: None,
                },
            ))
            .await;
    });

    env.new_string(transfer_id.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_app_core_NativeNode_updateResources<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    resources_json_jstr: jni::objects::JString<'local>,
) {
    let json: String = match env.get_string(&resources_json_jstr) {
        Ok(s) => s.into(),
        Err(_) => return,
    };

    if let Ok(resources) = serde_json::from_str::<Vec<nodeinnet_p2p::p2p::SharedResource>>(&json) {
        if let Ok(tx_guard) = NET_TX.lock() {
            if let Some(tx) = tx_guard.as_ref() {
                let tx_clone = tx.clone();
                RUNTIME.spawn(async move {
                    let _ = tx_clone
                        .send(crate::NetCmd::ReloadResources(resources))
                        .await;
                });
            }
        }
    }
}
