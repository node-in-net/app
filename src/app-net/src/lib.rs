use app_core::desktop::DesktopRouter;
use app_core::fm::ResourceTab;
use app_core::fm_core::rpc::FileSystemRpc;
use app_core::remote_fs::{PendingP2p, PendingTransfers, RemotePeerFsRpc};
use app_core::session::{AuthRpc, RestoredSession};
use app_core::terminal::TerminalRouter;
use app_core::webvpn::WebVpnRouter;
pub use client_core::{NetCmd, P2pPeerState};
pub use nodeinnet_p2p::TurnRegion;
use nodeinnet_p2p::{NodeInfo, P2pMessage, ResourceType};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

mod launch_provider;
pub use launch_provider::HostedLauncher;
mod launcher;
pub use launcher::{launch_proxied_app, LaunchedApp, SocksManager, TunnelTotals, PORT_VAR};
mod socks;
pub use socks::SocksProxy;

pub struct Routers {
    pub pending: Arc<PendingP2p>,
    pub transfers: Arc<PendingTransfers>,
    pub terminal: Arc<TerminalRouter>,
    pub desktop: Arc<DesktopRouter>,
    pub webvpn: Arc<WebVpnRouter>,
    pub remote_apps: Arc<app_core::remote_apps::RemoteAppsCache>,
    pub sysinfo: Arc<app_core::sysinfo::SysInfoCache>,
}

impl Routers {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Arc::new(PendingP2p::default()),
            transfers: Arc::new(PendingTransfers::default()),
            terminal: TerminalRouter::new(),
            desktop: DesktopRouter::new(),
            webvpn: WebVpnRouter::new(),
            remote_apps: app_core::remote_apps::RemoteAppsCache::new(),
            sysinfo: app_core::sysinfo::SysInfoCache::new(),
        })
    }

    pub fn route(&self, msg: P2pMessage) -> Option<P2pMessage> {
        if let P2pMessage::RemoteDesktopResponse {
            width: Some(w),
            height: Some(h),
            ..
        } = &msg
        {
            self.desktop.set_screen_size(*w, *h);
        }
        let msg = self.pending.resolve(msg)?;
        let msg = web_davserver::feed_response(msg)?;
        let msg = self.terminal.route(msg)?;
        let msg = self.webvpn.route(msg)?;
        let msg = self.remote_apps.route(msg)?;
        let msg = self.sysinfo.route(msg)?;
        Some(msg)
    }
}

#[derive(Debug, Clone)]
pub enum NetEvent {
    Nodes(Vec<NodeInfo>),
    PeerState(String, P2pPeerState),
    PeerGone(String),
    Ws(bool),
}

struct Handler {
    routers: Arc<Routers>,
    events: Box<dyn Fn(NetEvent) + Send + Sync>,
}

#[async_trait::async_trait]
impl client_core::AppEventHandler for Handler {
    async fn on_log(&self, msg: String) {
        if std::env::var_os("NODEINNET_P2P_LOG").is_some() {
            eprintln!("[p2p] · {msg}");
        }
    }
    async fn on_connected(&self) {
        (self.events)(NetEvent::Ws(true));
    }
    async fn on_disconnected(&self) {
        (self.events)(NetEvent::Ws(false));
    }
    async fn on_ws_state_changed(&self, state: client_core::WsState) {
        (self.events)(NetEvent::Ws(matches!(
            state,
            client_core::WsState::Connected
        )));
    }
    async fn on_update_nodes(&self, nodes: Vec<NodeInfo>) {
        (self.events)(NetEvent::Nodes(nodes));
    }
    async fn on_download_complete(&self, _path: std::path::PathBuf) {}
    async fn on_p2p_message(&self, msg: P2pMessage) {
        let _ = self.routers.route(msg);
    }
    async fn on_local_p2p_event(&self, event: p2p_node::LocalP2pEvent) {
        match event {
            p2p_node::LocalP2pEvent::RemoteDesktopFrame {
                width,
                height,
                bgra_data,
                compressed_size,
                ..
            } => {
                self.routers.desktop.record_frame(
                    width as u32,
                    height as u32,
                    compressed_size,
                    &bgra_data,
                );
            }
            p2p_node::LocalP2pEvent::RemoteDesktopStopped { .. } => {
                self.routers.desktop.record_stop();
            }
            p2p_node::LocalP2pEvent::TransferProgress {
                transfer_id,
                bytes_read,
                total_bytes,
                ..
            } => {
                self.routers
                    .transfers
                    .progress(transfer_id, bytes_read, total_bytes);
            }
            p2p_node::LocalP2pEvent::TransferComplete {
                transfer_id,
                is_upload: false,
                ..
            } => {
                self.routers.transfers.complete(transfer_id, Ok(()));
            }
            _ => {}
        }
    }
    async fn on_p2p_connected(&self, peer_id: String) {
        (self.events)(NetEvent::PeerState(peer_id, P2pPeerState::Connected));
    }
    async fn on_p2p_disconnected(&self, peer_id: String) {
        (self.events)(NetEvent::PeerState(
            peer_id.clone(),
            P2pPeerState::Disconnected,
        ));
        (self.events)(NetEvent::PeerGone(peer_id));
    }
    async fn on_peer_state_changed(&self, peer_id: String, state: P2pPeerState) {
        (self.events)(NetEvent::PeerState(peer_id, state));
    }
    async fn on_peer_failed(&self, peer_id: String, failure: client_core::P2pFailure) {
        eprintln!("[p2p] ✗ {peer_id}: {failure}");
    }
}

fn make_resource(
    id: &str,
    name: &str,
    resource_type: ResourceType,
    config: Option<String>,
) -> nodeinnet_p2p::SharedResource {
    nodeinnet_p2p::SharedResource {
        id: id.to_string(),
        name: name.to_string(),
        resource_type,
        config,
        is_active: true,
        session_token: None,
    }
}

fn default_resource_for(rt: ResourceType) -> Option<nodeinnet_p2p::SharedResource> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    Some(match rt {
        ResourceType::Filesystem => make_resource(
            "fs-home",
            "Home",
            ResourceType::Filesystem,
            Some(serde_json::json!({ "shares": [{ "name": "Home", "path": home }] }).to_string()),
        ),
        ResourceType::Terminal => make_resource("terminal", "Shell", ResourceType::Terminal, None),
        ResourceType::RemoteDesktop => {
            make_resource("desktop", "Screen", ResourceType::RemoteDesktop, None)
        }
        ResourceType::SharedNetwork => {
            make_resource("network", "Internet", ResourceType::SharedNetwork, None)
        }
        ResourceType::Registry => {
            make_resource("registry", "Registry", ResourceType::Registry, None)
        }
        ResourceType::SystemInfo => make_resource(
            "sysinfo",
            "System information",
            ResourceType::SystemInfo,
            None,
        ),
        ResourceType::SyncFolder => return None,
    })
}

pub fn resources_for(
    files: bool,
    terminal: bool,
    desktop: bool,
    network: bool,
) -> Vec<nodeinnet_p2p::SharedResource> {
    [
        (files, ResourceType::Filesystem),
        (terminal, ResourceType::Terminal),
        (desktop, ResourceType::RemoteDesktop),
        (network, ResourceType::SharedNetwork),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .filter_map(|(_, rt)| default_resource_for(rt))
    .collect()
}

pub fn default_shared_resources(
    config: &client_config::AppConfig,
) -> Vec<nodeinnet_p2p::SharedResource> {
    let mut resources = config
        .get::<Vec<nodeinnet_p2p::SharedResource>>("app.resources")
        .unwrap_or_default();
    normalize_resource_ids(&mut resources, config);
    resources
}

fn normalize_resource_ids(
    resources: &mut [nodeinnet_p2p::SharedResource],
    config: &client_config::AppConfig,
) {
    let Some(node_id) = config.get::<String>("app.node_id") else {
        return;
    };
    for r in resources.iter_mut() {
        let Some(base) = nodeinnet_p2p::resource_id_base(&r.resource_type) else {
            continue;
        };
        r.id = format!("{base}-{node_id}");
    }
}

pub fn store_shared_services(
    config: &client_config::AppConfig,
    resources: &[nodeinnet_p2p::SharedResource],
) {
    config.set("app.resources", resources.to_vec());
    config.save();
}

pub fn default_fs_resource() -> nodeinnet_p2p::SharedResource {
    default_resource_for(ResourceType::Filesystem).expect("the filesystem arm always yields one")
}

pub fn fs_resource_from_shares(shares: &[(String, String)]) -> nodeinnet_p2p::SharedResource {
    let list: Vec<_> = shares
        .iter()
        .map(|(name, path)| serde_json::json!({ "name": name, "path": path }))
        .collect();
    nodeinnet_p2p::SharedResource {
        id: "fs-home".into(),
        name: "Files".into(),
        resource_type: ResourceType::Filesystem,
        config: Some(serde_json::json!({ "shares": list }).to_string()),
        is_active: true,
        session_token: None,
    }
}

fn kind_to_rt(kind: app_core::workspace::ServiceKind) -> ResourceType {
    use app_core::workspace::ServiceKind;
    match kind {
        ServiceKind::Files => ResourceType::Filesystem,
        ServiceKind::Terminal => ResourceType::Terminal,
        ServiceKind::Desktop => ResourceType::RemoteDesktop,
        ServiceKind::Network => ResourceType::SharedNetwork,
        ServiceKind::Registry => ResourceType::Registry,
        ServiceKind::SystemInfo => ResourceType::SystemInfo,
    }
}

pub fn default_resource_for_kind(
    kind: app_core::workspace::ServiceKind,
) -> Option<nodeinnet_p2p::SharedResource> {
    default_resource_for(kind_to_rt(kind))
}

pub fn set_service(
    config: &client_config::AppConfig,
    kind: app_core::workspace::ServiceKind,
    on: bool,
) {
    let rt = kind_to_rt(kind);
    let mut resources = default_shared_resources(config);
    resources.retain(|r| r.resource_type != rt);
    if on {
        if let Some(res) = default_resource_for(rt) {
            resources.push(res);
        }
    }
    store_shared_services(config, &resources);
}

pub fn is_shared(
    config: &client_config::AppConfig,
    kind: app_core::workspace::ServiceKind,
) -> bool {
    let rt = kind_to_rt(kind);
    default_shared_resources(config)
        .iter()
        .any(|r| r.resource_type == rt && r.is_active)
}

pub fn shared_services(config: &client_config::AppConfig) -> Vec<app_core::workspace::ServiceKind> {
    use app_core::workspace::ServiceKind as K;
    [
        K::Files,
        K::Terminal,
        K::Desktop,
        K::Network,
        K::Registry,
        K::SystemInfo,
    ]
    .into_iter()
    .filter(|k| is_shared(config, *k))
    .collect()
}

pub fn current_shares(config: &client_config::AppConfig) -> Vec<(String, String)> {
    default_shared_resources(config)
        .iter()
        .find(|r| r.resource_type == ResourceType::Filesystem && r.is_active)
        .and_then(|r| r.config.as_ref())
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get("shares").and_then(|s| s.as_array()).cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    Some((
                        s.get("name")?.as_str()?.to_string(),
                        s.get("path")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn set_file_shares(config: &client_config::AppConfig, shares: &[(String, String)]) {
    let mut resources = default_shared_resources(config);
    resources.retain(|r| r.resource_type != ResourceType::Filesystem);
    if !shares.is_empty() {
        resources.push(fs_resource_from_shares(shares));
    }
    store_shared_services(config, &resources);
}

pub fn default_registry_resource() -> nodeinnet_p2p::SharedResource {
    nodeinnet_p2p::SharedResource {
        id: "registry".into(),
        name: "Registry".into(),
        resource_type: ResourceType::Registry,
        config: None,
        is_active: true,
        session_token: None,
    }
}

pub fn is_registry_shared(config: &client_config::AppConfig) -> bool {
    default_shared_resources(config)
        .iter()
        .any(|r| r.resource_type == ResourceType::Registry && r.is_active)
}

pub fn set_registry_shared(config: &client_config::AppConfig, on: bool) {
    let mut resources = default_shared_resources(config);
    resources.retain(|r| r.resource_type != ResourceType::Registry);
    if on {
        resources.push(default_registry_resource());
    }
    store_shared_services(config, &resources);
}

pub struct Identity {
    pub my_info: NodeInfo,
    pub private_key: String,
    pub config: client_config::AppConfig,
}

impl Identity {
    pub fn load_or_create(app_name: &str, display_name: &str) -> Identity {
        let config = client_config::AppConfig::new(app_name);
        let (private_key, public_key) = match (
            config.get::<String>("app.private_key_b64"),
            config.get::<String>("app.public_key_b64"),
        ) {
            (Some(private), Some(public)) => (private, public),
            _ => {
                let (private, public) = nodeinnet_p2p::crypto::generate_ed25519_keypair();
                config.set("app.private_key_b64", private.clone());
                config.set("app.public_key_b64", public.clone());
                config.save();
                (private, public)
            }
        };
        let node_id = match config.get::<String>("app.node_id") {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                config.set("app.node_id", id.clone());
                config.save();
                id
            }
        };
        let my_info = NodeInfo {
            id: node_id.clone(),
            name: display_name.to_string(),
            os: std::env::consts::OS.to_string(),
            version: app_version::APP_VERSION.to_string(),
            app_type: app_version::APP_TYPE.to_string(),
            build_type: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_string(),
            public_key,
            resources: default_shared_resources(&config),
            is_online: true,
            last_used: 0,
            is_temporary: false,
        };
        Identity {
            my_info,
            private_key,
            config,
        }
    }
}

pub struct Net {
    pub net_tx: tokio::sync::mpsc::Sender<NetCmd>,
    pub socks: Arc<SocksManager>,
    pub routers: Arc<Routers>,
    pub my_info: NodeInfo,
    pub config: client_config::AppConfig,
}

impl std::fmt::Debug for Net {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Net")
            .field("node_id", &self.my_info.id)
            .finish_non_exhaustive()
    }
}

impl Net {
    pub fn spawn(
        identity: Identity,
        routers: Arc<Routers>,
        events: impl Fn(NetEvent) + Send + Sync + 'static,
    ) -> Net {
        let (net_tx, net_rx) = tokio::sync::mpsc::channel(256);
        let handler = Arc::new(Handler {
            routers: routers.clone(),
            events: Box::new(events),
        });

        p2p_node::set_app_version(app_version::APP_VERSION);
        let socks = Arc::new(SocksManager::new());
        p2p_handlers::install(
            p2p_handlers::Capabilities::ALL,
            p2p_handlers::HostSettings {
                screencast_restore_token: identity
                    .config
                    .get::<String>(client_config::SCREENCAST_TOKEN_KEY),
                on_screencast_restore_token: Some({
                    let config = identity.config.clone();
                    Arc::new(move |token: String| {
                        config.set(client_config::SCREENCAST_TOKEN_KEY, token);
                        config.save();
                    })
                }),
                app_launcher: Some(Arc::new(launch_provider::HostedLauncher::new(
                    socks.clone(),
                    identity.config.clone(),
                    net_tx.clone(),
                    routers.webvpn.clone(),
                ))),
            },
        );

        client_core::network::spawn_network_task(
            net_rx,
            net_tx.clone(),
            handler,
            identity.my_info.clone(),
            identity.private_key.clone(),
            std::sync::Arc::new(client_config::ConfigPeerStore::new(identity.config.clone())),
            identity
                .config
                .get::<bool>(client_config::LOCAL_DISCOVERY_KEY)
                .unwrap_or(false),
        );
        Net {
            net_tx,
            socks,
            routers,
            my_info: identity.my_info,
            config: identity.config,
        }
    }

    pub fn node_id(&self) -> &str {
        &self.my_info.id
    }

    pub fn connect(
        &self,
        ws_base: &str,
        access_token: &str,
        turn: Option<nodeinnet_p2p::rtc::TurnCredentials>,
    ) {
        let sep = if ws_base.contains('?') { '&' } else { '?' };
        let url = format!(
            "{ws_base}{sep}token={access_token}&session_id={}",
            self.my_info.id
        );
        let _ = self
            .net_tx
            .try_send(NetCmd::Connect(url, self.my_info.clone(), turn));
    }

    pub fn call_peer(&self, peer_id: impl Into<String>) {
        let _ = self.net_tx.try_send(NetCmd::Call(peer_id.into()));
    }

    pub fn sender_to(&self, peer_id: impl Into<String>) -> impl Fn(P2pMessage) + 'static {
        let tx = self.net_tx.clone();
        let peer_id = peer_id.into();
        move |msg| {
            if let Err(e) = tx.try_send(NetCmd::SendToPeer(peer_id.clone(), msg)) {
                eprintln!("[net] ✗ dropped a message for {peer_id}: {e}");
            }
        }
    }

    pub fn peer_p2p_sender(
        &self,
        peer_id: impl Into<String>,
    ) -> tokio::sync::mpsc::Sender<P2pMessage> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<P2pMessage>(32);
        let net_tx = self.net_tx.clone();
        let peer = peer_id.into();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let _ = net_tx.send(NetCmd::SendToPeer(peer.clone(), msg)).await;
            }
        });
        tx
    }

    pub fn media_sender(
        &self,
        peer_id: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> impl Fn(bool, app_core::desktop::StreamOptions) + 'static {
        let tx = self.net_tx.clone();
        let peer_id = peer_id.into();
        let resource_id = resource_id.into();
        move |start, opts| {
            let _ = tx.try_send(NetCmd::SendToPeer(
                peer_id.clone(),
                P2pMessage::RemoteDesktopRequest {
                    resource_id: resource_id.clone(),
                    start,
                    original_size: opts.original_size,
                    bitrate_bps: opts.bitrate_bps,
                    force_select: opts.force_select,
                },
            ));
        }
    }
}

const FS_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub fn fs_tabs(net: &Net, node: &NodeInfo) -> Vec<(ResourceTab, Rc<dyn FileSystemRpc>)> {
    node.resources
        .iter()
        .filter(|r| r.is_active && r.resource_type == ResourceType::Filesystem)
        .map(|r| {
            let rpc = RemotePeerFsRpc::new(
                r.id.clone(),
                net.routers.pending.clone(),
                net.routers.transfers.clone(),
                net.sender_to(node.id.clone()),
            )
            .with_bulk_sender(net.peer_p2p_sender(node.id.clone()))
            .with_timeout(FS_REQUEST_TIMEOUT);
            (
                ResourceTab {
                    id: r.id.clone(),
                    label: r.name.clone(),
                },
                Rc::new(rpc) as Rc<dyn FileSystemRpc>,
            )
        })
        .collect()
}

pub fn first_resource(node: &NodeInfo, kind: ResourceType) -> Option<String> {
    node.resources
        .iter()
        .find(|r| r.is_active && r.resource_type == kind)
        .map(|r| r.id.clone())
}

type RegistrySink = Rc<dyn Fn(Vec<NodeInfo>)>;

pub fn remote_apps_rpc(net: &Net, node: &NodeInfo) -> Option<app_core::remote_apps::RemoteAppsRpc> {
    let res = node
        .resources
        .iter()
        .find(|r| r.is_active && r.resource_type == ResourceType::SharedNetwork)?;
    Some(app_core::remote_apps::RemoteAppsRpc::new(
        res.id.clone(),
        net.routers.pending.clone(),
        net.sender_to(node.id.clone()),
    ))
}

pub fn sysinfo_rpc(net: &Net, node: &NodeInfo) -> Option<app_core::sysinfo::SysInfoRpc> {
    let res = node
        .resources
        .iter()
        .find(|r| r.is_active && r.resource_type == ResourceType::SystemInfo)?;
    Some(app_core::sysinfo::SysInfoRpc::new(
        res.id.clone(),
        net.routers.sysinfo.clone(),
        net.sender_to(node.id.clone()),
    ))
}

pub fn registry_rpc(
    net: &Net,
    node: &NodeInfo,
) -> Option<app_core::registry::RemotePeerRegistryRpc> {
    let res = node
        .resources
        .iter()
        .find(|r| r.is_active && r.resource_type == ResourceType::Registry)?;
    Some(app_core::registry::RemotePeerRegistryRpc::new(
        res.id.clone(),
        net.routers.pending.clone(),
        net.sender_to(node.id.clone()),
    ))
}

#[derive(serde::Deserialize)]
struct DeviceRow {
    name: String,
    hostname: Option<String>,
    os: Option<String>,
    app_version: Option<String>,
    app_type: Option<String>,
    build_type: Option<String>,
    is_online: bool,
    last_used: Option<i64>,
    #[serde(default)]
    resources: Vec<nodeinnet_p2p::SharedResource>,
}

pub async fn fetch_registry(api_target: &str, access_token: &str) -> Result<Vec<NodeInfo>, String> {
    let rows: Vec<DeviceRow> = reqwest::Client::new()
        .get(format!("{api_target}/account/devices"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|d| NodeInfo {
            id: d.name.clone(),
            name: d.hostname.unwrap_or(d.name),
            os: d.os.unwrap_or_default(),
            version: d.app_version.unwrap_or_default(),
            app_type: d.app_type.unwrap_or_default(),
            build_type: d.build_type.unwrap_or_default(),
            public_key: String::new(),
            resources: d.resources,
            is_online: d.is_online,
            last_used: d.last_used.unwrap_or(0),
            is_temporary: false,
        })
        .collect())
}

const GUEST_KEY: &str = "app.guest_session";

pub struct ServerAuth {
    api_target: String,
    ws_override: Option<String>,
    net_tx: tokio::sync::mpsc::Sender<NetCmd>,
    my_info: NodeInfo,
    config: client_config::AppConfig,
    session: RefCell<Option<nodeinnet_p2p::LoginResponse>>,
    registry_sink: RefCell<Option<RegistrySink>>,
    temporary: std::cell::Cell<bool>,
}

impl ServerAuth {
    pub fn new(net: &Net, api_target: impl Into<String>, ws_override: Option<String>) -> Self {
        Self {
            api_target: api_target.into(),
            ws_override,
            net_tx: net.net_tx.clone(),
            my_info: net.my_info.clone(),
            config: net.config.clone(),
            session: RefCell::new(None),
            registry_sink: RefCell::new(None),
            temporary: std::cell::Cell::new(net.config.get::<bool>(GUEST_KEY).unwrap_or(false)),
        }
    }

    async fn set_turn_region_inner(&self, region: nodeinnet_p2p::TurnRegion) -> Result<(), String> {
        let access_token = {
            let s = self.session.borrow();
            let s = s.as_ref().ok_or("not signed in")?;
            s.access_token.clone()
        };

        #[derive(serde::Deserialize)]
        struct Reply {
            turn_credentials: Option<nodeinnet_p2p::rtc::TurnCredentials>,
        }

        let reply: Reply = reqwest::Client::new()
            .post(format!("{}/account/region", self.api_target))
            .bearer_auth(&access_token)
            .json(&serde_json::json!({ "region": region.as_str() }))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

        self.config.set_turn_region(region);
        if let Some(s) = self.session.borrow_mut().as_mut() {
            s.turn = reply.turn_credentials.clone();
        }
        let _ = self
            .net_tx
            .send(NetCmd::ApplyTurnCredentials(reply.turn_credentials))
            .await;
        Ok(())
    }

    pub fn set_registry_sink(&self, sink: impl Fn(Vec<NodeInfo>) + 'static) {
        *self.registry_sink.borrow_mut() = Some(Rc::new(sink));
    }

    fn connect_node(
        &self,
        ws_url: String,
        access_token: &str,
        turn: Option<nodeinnet_p2p::rtc::TurnCredentials>,
    ) {
        let ws_base = self.ws_override.clone().unwrap_or(ws_url);
        let sep = if ws_base.contains('?') { '&' } else { '?' };
        let url = format!(
            "{ws_base}{sep}token={access_token}&session_id={}",
            self.my_info.id
        );
        let mut my_info = self.my_info.clone();
        my_info.resources = default_shared_resources(&self.config);
        my_info.is_temporary = self.temporary.get();
        let _ = self.net_tx.try_send(NetCmd::Connect(url, my_info, turn));

        match self.registry_sink.borrow().clone() {
            Some(sink) => {
                let api = self.api_target.clone();
                let token = access_token.to_string();
                tokio::task::spawn_local(async move {
                    match fetch_registry(&api, &token).await {
                        Ok(nodes) => sink(nodes),
                        Err(e) => eprintln!("device registry fetch failed: {e}"),
                    }
                });
            }
            None => eprintln!("device registry: no sink wired"),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl AuthRpc for ServerAuth {
    async fn set_turn_region(&self, region: nodeinnet_p2p::TurnRegion) -> Result<(), String> {
        self.set_turn_region_inner(region).await
    }

    async fn login(&self, login: String, password: String) -> Result<(), String> {
        let resp = client_core::auth::login(
            &self.api_target,
            &login,
            &password,
            self.config.turn_region(),
        )
        .await?;
        self.config
            .set("app.refresh_token", resp.refresh_token.clone());
        self.config.set("app.account_login", login);
        self.config.set_turn_region(resp.turn_region);
        self.config.save();
        *self.session.borrow_mut() = Some(resp);
        Ok(())
    }

    async fn register_device(&self, node_id: String, device_name: String) -> Result<(), String> {
        let (access_token, ws_url, turn) = {
            let session = self.session.borrow();
            let s = session.as_ref().ok_or("not logged in")?;
            (s.access_token.clone(), s.ws_url.clone(), s.turn.clone())
        };
        let profile = client_core::auth::DeviceProfile {
            display_name: Some(device_name),
            os: Some(std::env::consts::OS.to_string()),
            app_type: Some(app_version::APP_TYPE.to_string()),
            version: Some(app_version::APP_VERSION.to_string()),
            resources: default_shared_resources(&self.config)
                .iter()
                .map(|r| r.without_config())
                .collect(),
        };
        client_core::auth::register_device(&self.api_target, &access_token, &node_id, &profile)
            .await?;

        self.temporary.set(false);
        self.config.set(GUEST_KEY, false);
        self.config.save();
        self.connect_node(ws_url, &access_token, turn);
        Ok(())
    }

    async fn join_temporary(&self) -> Result<(), String> {
        let (access_token, ws_url, turn) = {
            let session = self.session.borrow();
            let s = session.as_ref().ok_or("not logged in")?;
            (s.access_token.clone(), s.ws_url.clone(), s.turn.clone())
        };
        self.temporary.set(true);
        self.config.set(GUEST_KEY, true);
        self.config.save();
        self.connect_node(ws_url, &access_token, turn);
        Ok(())
    }

    async fn restore(&self) -> Result<RestoredSession, String> {
        let refresh_token = self
            .config
            .get::<String>("app.refresh_token")
            .filter(|t| !t.is_empty())
            .ok_or("no stored refresh token")?;
        let account_login = self.config.get::<String>("app.account_login");
        let resp = match client_core::auth::refresh_access_token(
            &self.api_target,
            &refresh_token,
            self.config.turn_region(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[net] ✗ session restore failed: {e}");
                return Err(e);
            }
        };
        self.config
            .set("app.refresh_token", resp.refresh_token.clone());
        self.config.set_turn_region(resp.turn_region);
        self.config.save();

        let known = resp.devices.iter().any(|d| d.name == self.my_info.id);
        if !known && !self.temporary.get() {
            eprintln!("[net] · this device is no longer registered — registering again");
            let profile = client_core::auth::DeviceProfile {
                display_name: Some(self.my_info.name.clone()),
                os: Some(std::env::consts::OS.to_string()),
                app_type: Some(app_version::APP_TYPE.to_string()),
                version: Some(app_version::APP_VERSION.to_string()),
                resources: default_shared_resources(&self.config)
                    .iter()
                    .map(|r| r.without_config())
                    .collect(),
            };
            if let Err(e) = client_core::auth::register_device(
                &self.api_target,
                &resp.access_token,
                &self.my_info.id,
                &profile,
            )
            .await
            {
                eprintln!("[net] ✗ re-registration failed: {e}");
            }
        }

        self.connect_node(resp.ws_url.clone(), &resp.access_token, resp.turn.clone());
        Ok(RestoredSession {
            node_id: self.my_info.id.clone(),
            device_name: self.my_info.name.clone(),
            account_login,
        })
    }

    async fn logout(&self) -> Result<(), String> {
        let refresh = self
            .config
            .get::<String>("app.refresh_token")
            .unwrap_or_default();
        if !refresh.is_empty() {
            let access = self
                .session
                .borrow()
                .as_ref()
                .map(|s| s.access_token.clone());
            if let Err(e) =
                client_core::auth::logoff(&self.api_target, &refresh, access.as_deref()).await
            {
                eprintln!("server logoff (best-effort) failed: {e}");
            }
        }
        let _ = self.net_tx.try_send(NetCmd::Disconnect);
        self.config.set("app.refresh_token", "");
        self.config.set("app.account_login", "");
        self.config.save();
        *self.session.borrow_mut() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodeinnet_p2p::{ResourceType, SharedResource};

    #[test]
    fn the_hosts_screen_size_is_taken_off_its_desktop_response() {
        let routers = Routers::new();
        assert_eq!(routers.desktop.screen_size(), None);

        let unclaimed = routers.route(P2pMessage::RemoteDesktopResponse {
            resource_id: "screen-1".into(),
            success: true,
            error_msg: None,
            width: Some(1920),
            height: Some(1080),
        });

        assert!(unclaimed.is_some(), "the response still travels on");
        assert_eq!(
            routers.desktop.screen_size(),
            Some((1920, 1080)),
            "without this the pointer maps onto a default screen and lands short"
        );
    }

    #[test]
    fn fs_resource_id_is_derived_from_node_id_not_the_shared_default() {
        let cfg = client_config::AppConfig::new("_app_net_norm_test_do_not_use");
        cfg.set("app.node_id", "NODE123".to_string());
        let res = |id: &str, rt: ResourceType| SharedResource {
            id: id.into(),
            name: id.into(),
            resource_type: rt,
            config: None,
            is_active: true,
            session_token: None,
        };
        cfg.set(
            "app.resources",
            vec![
                res("fs-home", ResourceType::Filesystem),
                res("terminal", ResourceType::Terminal),
                res("network", ResourceType::SharedNetwork),
            ],
        );
        let out = default_shared_resources(&cfg);
        assert_eq!(out[0].id, "fs-NODE123");
        assert_eq!(out[1].id, "terminal-NODE123");
        assert_eq!(out[2].id, "network-NODE123");
    }

    #[tokio::test]
    #[ignore]
    async fn live_restore_diagnostic() {
        let api =
            std::env::var("NODEINNET_API").unwrap_or_else(|_| "https://node.in.net".to_string());
        let identity = Identity::load_or_create("app", "diag");
        let node_id = identity.my_info.id.clone();
        let routers = Routers::new();
        let net = Net::spawn(identity, routers, |_| {});
        let auth = ServerAuth::new(&net, api.clone(), std::env::var("NODEINNET_WS").ok());
        eprintln!("[diag] api={api}  node_id={node_id}");
        match auth.restore().await {
            Ok(r) => eprintln!(
                "[diag] RESTORE OK: account={:?} device={}",
                r.account_login, r.device_name
            ),
            Err(e) => eprintln!("[diag] RESTORE FAILED: {e}"),
        }
    }
}

#[cfg(test)]
mod session_end_tests {
    use super::*;
    use client_core::AppEventHandler;
    use std::sync::Mutex;

    fn handler_over(seen: Arc<Mutex<Vec<NetEvent>>>) -> Handler {
        Handler {
            routers: Routers::new(),
            events: Box::new(move |e| seen.lock().unwrap().push(e)),
        }
    }

    #[tokio::test]
    async fn a_transport_wobble_does_not_end_the_session() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let h = handler_over(seen.clone());

        h.on_peer_state_changed("peer-a".into(), P2pPeerState::Disconnected)
            .await;

        let events = seen.lock().unwrap();
        assert_eq!(events.len(), 1, "one event, not a teardown");
        assert!(matches!(events[0], NetEvent::PeerState(_, _)));
        assert!(
            !events.iter().any(|e| matches!(e, NetEvent::PeerGone(_))),
            "a wobble must not free the peer's tunnels"
        );
    }

    #[tokio::test]
    async fn a_closed_session_is_announced_as_gone() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let h = handler_over(seen.clone());

        h.on_p2p_disconnected("peer-a".into()).await;

        let events = seen.lock().unwrap();
        assert_eq!(events.len(), 2, "the dot greys AND the session ends");
        assert!(matches!(
            events[0],
            NetEvent::PeerState(_, P2pPeerState::Disconnected)
        ));
        match &events[1] {
            NetEvent::PeerGone(id) => assert_eq!(id, "peer-a"),
            other => panic!("expected PeerGone, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod resource_id_tests {
    use super::*;
    use app_core::workspace::ServiceKind;

    const SERVICES: [ServiceKind; 6] = [
        ServiceKind::Files,
        ServiceKind::Terminal,
        ServiceKind::Desktop,
        ServiceKind::Network,
        ServiceKind::Registry,
        ServiceKind::SystemInfo,
    ];

    #[test]
    fn every_service_provisions_a_resource() {
        for kind in SERVICES {
            assert!(
                default_resource_for_kind(kind).is_some(),
                "{kind:?} can be switched on but provisions nothing"
            );
        }
    }

    #[test]
    fn every_provisioned_resource_has_a_canonical_id() {
        for kind in SERVICES {
            let res = default_resource_for_kind(kind).expect("provisioned above");
            let base = nodeinnet_p2p::resource_id_base(&res.resource_type)
                .unwrap_or_else(|| panic!("{kind:?} has no canonical id base"));
            assert!(
                res.id == base || res.id.starts_with(&format!("{base}-")),
                "{kind:?} is built as {:?} but normalises to {base}-{{node}}",
                res.id
            );
        }
    }
}
