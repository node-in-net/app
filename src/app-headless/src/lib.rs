use actix::{Actor, AsyncContext, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use app_core::desktop::{Desktop, DesktopRouter};
use app_core::fm::{FileManager, PanelState, Side};
use app_core::fm_core::rpc::FileSystemRpc;
use app_core::session::Session;
use app_core::terminal::{Terminal, TerminalRouter};
use app_core::webvpn::{FetchResponse, WebVpn};
use app_core::workspace::{RailViewMode, ServiceKind, Theme, Workspace};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::BroadcastStream;

#[derive(Default)]
pub struct App {
    pub workspace: Workspace,
    pub fm: FileManager,
    pub terminal: Terminal,
    pub desktop: Desktop,
    pub webvpn: WebVpn,
    pub reg: app_core::registry::Registry,
    pub sysinfo: app_core::sysinfo::SystemInfo,
    pub session: Session,
    pub net: Option<app_net::Net>,
    pub nodes: Vec<nodeinnet_p2p::NodeInfo>,
    pub peer_links: std::collections::HashMap<String, app_core::workspace::LinkState>,
    pub registry: Vec<nodeinnet_p2p::NodeInfo>,
    pub self_cmd: Option<mpsc::UnboundedSender<ApiCmd>>,
    pub setup_identity: Option<(String, String)>,
    pub visible_page: Option<String>,
    pub wizard_fields: std::collections::BTreeMap<String, String>,
    pub registry_view: Option<serde_json::Value>,
    pub socks: std::sync::Arc<app_net::SocksManager>,
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferOrigin {
    OtherPane(app_core::fm::TransferKind),
    Clipboard,
}

impl From<TransferOrigin> for app_core::fm::TransferFrom {
    fn from(o: TransferOrigin) -> Self {
        match o {
            TransferOrigin::OtherPane(kind) => Self::OtherPane(kind),
            TransferOrigin::Clipboard => Self::Clipboard,
        }
    }
}

#[derive(Debug)]
pub enum ApiCmd {
    SelectDevice {
        id: String,
    },
    CloseTab {
        id: String,
    },
    OpenService {
        kind: ServiceKind,
    },
    SysInfoRefresh,
    FocusService {
        kind: ServiceKind,
    },
    CloseService {
        kind: ServiceKind,
    },
    MapPeek,
    MapFull,
    MapClose,
    SetRailMode {
        mode: RailViewMode,
    },
    SetTheme {
        theme: Theme,
    },
    SetTurnRegion {
        region: nodeinnet_p2p::TurnRegion,
    },
    FmSelectResource {
        side: Side,
        index: usize,
    },
    FmNavigate {
        side: Side,
        path: String,
    },
    FmEnter {
        side: Side,
    },
    FmUp {
        side: Side,
    },
    FmBreadcrumb {
        side: Side,
        index: usize,
    },
    FmBack {
        side: Side,
    },
    FmForward {
        side: Side,
    },
    FmRefresh {
        side: Side,
    },
    FmActivate {
        side: Side,
    },
    FmCursor {
        side: Side,
        index: usize,
    },
    FmToggleSelect {
        side: Side,
        index: usize,
    },
    FmMkdir {
        side: Side,
        name: String,
    },
    FmDelete {
        side: Side,
    },
    FmRename {
        side: Side,
        new_name: String,
    },
    FmChmod {
        side: Side,
        path: String,
        mode: u32,
    },
    FmCopy {
        side: Side,
    },
    FmClipboardSet {
        side: Side,
        kind: app_core::fm::TransferKind,
    },
    FmClipboardClear,
    FmTransferPlan {
        dest: Side,
        from: TransferOrigin,
        into: Option<String>,
        reply: oneshot::Sender<app_core::fm::TransferPlan>,
    },
    FmTransferRun {
        dest: Side,
        from: TransferOrigin,
        into: Option<String>,
        resolutions: Vec<(String, app_core::fm::OnConflict)>,
    },
    FmMove {
        side: Side,
    },
    FmDuplicate {
        side: Side,
        path: String,
        new_name: String,
    },
    FmUpload {
        side: Side,
        dir: String,
        files: Vec<std::path::PathBuf>,
    },
    FmMount {
        side: Side,
    },
    FmUnmount {
        side: Side,
    },
    UnmountResource {
        resource_id: String,
    },
    FmSetLocalResource {
        side: Side,
        path: String,
    },
    TerminalStart,
    TerminalStop,
    TerminalResize {
        rows: u16,
        cols: u16,
    },
    TerminalInput {
        data: Vec<u8>,
    },
    SetupBegin,
    SetupName {
        name: String,
    },
    EnterWorkspace,
    SetAllowLocalNetwork {
        allowed: bool,
    },
    SetSharedServices {
        files: bool,
        terminal: bool,
        desktop: bool,
        network: bool,
    },
    ReloadSharedServices,
    ReportVisiblePage {
        name: String,
    },
    ReportRegistryView {
        path: String,
        subkeys_shown: u32,
        values_shown: u32,
        expanded: bool,
    },
    ReportField {
        field: String,
        value: String,
    },
    SetWizardField {
        field: String,
        value: String,
    },
    SetWizardService {
        kind: ServiceKind,
        on: bool,
    },
    WizardSubmit,
    Screenshot {
        path: String,
    },
    AuthLogin {
        login: String,
        password: String,
        guest: bool,
    },
    AuthGuest,
    AuthLogout,
    AuthRestore {
        reply: oneshot::Sender<Result<app_core::session::SessionState, String>>,
    },
    DesktopConnect {
        connect: bool,
        opts: app_core::desktop::StreamOptions,
    },
    DesktopControl {
        enabled: bool,
    },
    DesktopInput {
        event: nodeinnet_p2p::DesktopInputEvent,
    },
    WebVpnStart,
    WebVpnStop,
    WebVpnAddApp {
        name: String,
        exec_cmd: String,
    },
    WebVpnRemoveApp {
        id: String,
    },
    ProxiedAppsChanged,
    WebVpnLaunch {
        exec_cmd: String,
    },

    RegistryRequestKeys {
        path: String,
    },
    RegistryCreateKey {
        parent_path: String,
        key_name: String,
    },
    RegistrySetValue {
        path: String,
        value_name: String,
        data: nodeinnet_p2p::p2p::RegistryValueData,
    },
    RegistryDeleteEntry {
        path: String,
        value_name: Option<String>,
        is_key: bool,
    },
    WebVpnFetch {
        method: String,
        url: String,
        reply: oneshot::Sender<Result<FetchResponse, String>>,
    },
    NetAttach {
        net: Box<app_net::Net>,
        api_target: String,
        ws_override: Option<String>,
    },
    NetNodes {
        nodes: Vec<nodeinnet_p2p::NodeInfo>,
    },
    NetWsState {
        connected: bool,
    },
    NetPeerState {
        peer: String,
        state: app_core::workspace::LinkState,
    },
    NetPeerGone {
        peer: String,
    },
    NetRegistry {
        nodes: Vec<nodeinnet_p2p::NodeInfo>,
    },
    QueryState {
        reply: oneshot::Sender<serde_json::Value>,
    },
    QueryPanel {
        side: Side,
        reply: oneshot::Sender<PanelState>,
    },
    QuerySession {
        reply: oneshot::Sender<app_core::session::SessionState>,
    },
    QueryWebVpn {
        reply: oneshot::Sender<app_core::webvpn::WebVpnState>,
    },
    QueryRegistry {
        reply: oneshot::Sender<app_core::registry::RegistryState>,
    },
    QueryProxiedApps {
        reply: oneshot::Sender<Vec<serde_json::Value>>,
    },
    QueryRemoteApps {
        reply: oneshot::Sender<Option<app_core::remote_apps::RemoteAppsView>>,
    },
    RemoteAppsRefresh,
    RemoteAppLaunchThere {
        app_id: String,
    },
    RemoteAppStopThere {
        session_id: uuid::Uuid,
    },
    QueryTunnels {
        reply: oneshot::Sender<(app_net::TunnelTotals, Vec<app_net::LaunchedApp>)>,
    },
    TerminateApp {
        pid: u32,
    },
    SetAppRemoteLaunch {
        id: String,
        allowed: bool,
    },
    CloseTunnels,
    ReapTick,
    QueryDeviceServices {
        id: String,
        reply: oneshot::Sender<Vec<String>>,
    },
    FmReadFile {
        side: Side,
        path: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    FmWriteFile {
        side: Side,
        path: String,
        content: Vec<u8>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    FmListDir {
        side: Side,
        path: String,
        reply: oneshot::Sender<Result<Vec<app_core::fm_core::rpc::RemoteFileEntry>, String>>,
    },
    FmProviderInfo {
        side: Side,
        reply: oneshot::Sender<Option<(bool, bool)>>,
    },

    QueryTerminalRouter {
        reply: oneshot::Sender<Arc<TerminalRouter>>,
    },
    QueryDesktopRouter {
        reply: oneshot::Sender<Arc<DesktopRouter>>,
    },
}

fn services_of(
    resources: &[nodeinnet_p2p::SharedResource],
) -> Vec<app_core::workspace::ServiceKind> {
    use app_core::workspace::ServiceKind;
    let mut out = Vec::new();
    for r in resources {
        if !r.is_active {
            continue;
        }
        let kind = match r.resource_type {
            nodeinnet_p2p::ResourceType::Filesystem => ServiceKind::Files,
            nodeinnet_p2p::ResourceType::Terminal => ServiceKind::Terminal,
            nodeinnet_p2p::ResourceType::RemoteDesktop => ServiceKind::Desktop,
            nodeinnet_p2p::ResourceType::SharedNetwork => ServiceKind::Network,
            nodeinnet_p2p::ResourceType::Registry => ServiceKind::Registry,
            nodeinnet_p2p::ResourceType::SystemInfo => ServiceKind::SystemInfo,
            _ => continue,
        };
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    out
}

fn sync_devices(app: &mut App) {
    let own = app.net.as_ref().map(|n| n.node_id().to_string());
    let mut merged: Vec<app_core::workspace::DeviceInfo> = app
        .registry
        .iter()
        .filter(|n| Some(&n.id) != own.as_ref())
        .map(|n| app_core::workspace::DeviceInfo {
            id: n.id.clone(),
            name: n.name.clone(),
            os: n.os.clone(),
            online: false,
            link: app.peer_links.get(&n.id).copied().unwrap_or_default(),
            services: services_of(&n.resources),
        })
        .collect();
    for live in app.nodes.iter().filter(|n| Some(&n.id) != own.as_ref()) {
        let link = app.peer_links.get(&live.id).copied().unwrap_or_default();
        match merged.iter_mut().find(|d| d.id == live.id) {
            Some(d) => {
                d.name = live.name.clone();
                d.os = live.os.clone();
                d.online = live.is_online;
                d.link = link;
                d.services = services_of(&live.resources);
            }
            None => merged.push(app_core::workspace::DeviceInfo {
                id: live.id.clone(),
                name: live.name.clone(),
                os: live.os.clone(),
                online: live.is_online,
                link,
                services: services_of(&live.resources),
            }),
        }
    }
    app.workspace.apply_snapshot(merged);
}

async fn apply(app: &mut App, cmd: ApiCmd) {
    use ApiCmd::*;
    match cmd {
        SelectDevice { id } => {
            if app.workspace.select_device(&id) {
                if let Some(net) = app.net.as_ref() {
                    net.call_peer(id.clone());
                }
                wire_selected_device(app, &id);
            }
        }
        CloseTab { id } => {
            let before = app.workspace.selected_device().map(|d| d.id.clone());
            if app.workspace.close_tab(&id) {
                let after = app.workspace.selected_device().map(|d| d.id.clone());
                if after != before {
                    match after {
                        Some(new_id) => wire_selected_device(app, &new_id),
                        None => unwire_services(app),
                    }
                }
            }
        }
        SysInfoRefresh => {
            app.sysinfo.refresh().await;
        }
        OpenService { kind } => {
            app.workspace.open_service(kind);
        }
        FocusService { kind } => {
            app.workspace.focus_service(kind);
        }
        CloseService { kind } => {
            app.workspace.close_service(kind);
        }
        MapPeek => app.workspace.map_toggle_peek(),
        MapFull => app.workspace.map_open_full(),
        MapClose => app.workspace.map_close(),
        SetRailMode { mode } => app.workspace.set_rail_mode(mode),
        SetTheme { theme } => app.workspace.set_theme(theme),
        SetTurnRegion { region } => {
            let _ = app.session.set_turn_region(region).await;
        }

        FmSelectResource { side, index } => {
            app.fm.select_resource(side, index).await;
        }
        FmNavigate { side, path } => {
            app.fm.navigate(side, &path).await;
        }
        FmEnter { side } => {
            app.fm.enter(side).await;
        }
        FmUp { side } => {
            app.fm.up(side).await;
        }
        FmBreadcrumb { side, index } => {
            app.fm.breadcrumb(side, index).await;
        }
        FmBack { side } => {
            app.fm.back(side).await;
        }
        FmForward { side } => {
            app.fm.forward(side).await;
        }
        FmRefresh { side } => {
            app.fm.refresh(side).await;
        }
        FmActivate { side } => app.fm.activate(side),
        FmCursor { side, index } => app.fm.set_cursor(side, index),
        FmToggleSelect { side, index } => app.fm.toggle_select(side, index),
        FmMkdir { side, name } => {
            app.fm.mkdir(side, &name).await;
        }
        FmDelete { side } => {
            app.fm.delete_selected(side).await;
        }
        FmRename { side, new_name } => {
            app.fm.rename_cursor(side, &new_name).await;
        }
        FmChmod { side, path, mode } => {
            app.fm.chmod(side, &path, mode).await;
        }
        FmCopy { side } => {
            app.fm.copy_to(side).await;
        }
        FmClipboardSet { side, kind } => {
            app.fm.set_clipboard(side, kind);
        }
        FmClipboardClear => {
            app.fm.clear_clipboard();
        }
        FmTransferPlan {
            dest,
            from,
            into,
            reply,
        } => {
            let plan = app.fm.plan_transfer(dest, from.into(), into).await;
            let _ = reply.send(plan);
        }
        FmTransferRun {
            dest,
            from,
            into,
            resolutions,
        } => {
            app.fm
                .run_transfer(dest, from.into(), into, resolutions)
                .await;
        }
        FmMove { side } => {
            app.fm.move_to(side).await;
        }
        FmDuplicate {
            side,
            path,
            new_name,
        } => {
            app.fm.duplicate(side, path, new_name).await;
        }
        FmUpload { side, dir, files } => {
            app.fm.upload_local(side, dir, files).await;
        }
        FmMount { side } => {
            mount_side(app, side);
        }
        FmUnmount { side } => {
            unmount_side(app, side);
        }
        UnmountResource { resource_id } => {
            web_davserver::unmount_resource(&resource_id);
            for side in [Side::Left, Side::Right] {
                let active_id = {
                    let p = app.fm.panel(side);
                    p.active_resource
                        .and_then(|i| p.resources.get(i))
                        .map(|t| t.id.clone())
                };
                if active_id.as_deref() == Some(resource_id.as_str()) {
                    app.fm.set_mounted(side, None);
                }
            }
            app.workspace.remove_mount(&resource_id);
        }
        FmSetLocalResource { side, path } => {
            use app_core::fm::ResourceTab;
            let tab = ResourceTab {
                id: "local".into(),
                label: "Local".into(),
            };
            let rpc = std::rc::Rc::new(app_core::local_fs::LocalFsRpc::new(path))
                as std::rc::Rc<dyn FileSystemRpc>;
            app.fm.set_resources(side, vec![(tab, rpc)]);
            app.fm.select_resource(side, 0).await;
        }
        TerminalStart => {
            app.terminal.start();
        }
        TerminalStop => {
            app.terminal.stop();
        }
        TerminalResize { rows, cols } => {
            app.terminal.resize(rows, cols);
        }
        TerminalInput { data } => {
            app.terminal.input(data);
        }

        SetupBegin => {
            if let Some((node_id, name)) = app.setup_identity.clone() {
                app.session.begin_setup(node_id, name);
            }
        }
        Screenshot { .. } | SetWizardField { .. } | SetWizardService { .. } | WizardSubmit => {}
        SetupName { name } => {
            app.session.confirm_device_name(&name);
        }
        EnterWorkspace => {
            app.session.enter_workspace();
        }
        SetAllowLocalNetwork { allowed } => {
            if let Some(net) = &app.net {
                let mut resources = app_net::default_shared_resources(&net.config);
                match resources
                    .iter_mut()
                    .find(|r| r.resource_type == nodeinnet_p2p::ResourceType::SharedNetwork)
                {
                    Some(res) => {
                        res.config = Some(json!({ "allow_local_network": allowed }).to_string());
                        app_net::store_shared_services(&net.config, &resources);
                        let _ = net
                            .net_tx
                            .try_send(app_net::NetCmd::ReloadResources(resources));
                    }
                    None => eprintln!("[net] ✗ the network service is not shared"),
                }
            }
        }
        SetSharedServices {
            files,
            terminal,
            desktop,
            network,
        } => {
            if let Some(net) = &app.net {
                let stored = app_net::default_shared_resources(&net.config);
                let controlled = [
                    nodeinnet_p2p::ResourceType::Filesystem,
                    nodeinnet_p2p::ResourceType::Terminal,
                    nodeinnet_p2p::ResourceType::RemoteDesktop,
                    nodeinnet_p2p::ResourceType::SharedNetwork,
                ];
                let mut resources: Vec<_> = stored
                    .iter()
                    .filter(|r| !controlled.contains(&r.resource_type))
                    .cloned()
                    .collect();
                if stored.is_empty() {
                    resources.extend(app_net::default_resource_for_kind(
                        app_core::workspace::ServiceKind::SystemInfo,
                    ));
                }
                resources.extend(app_net::resources_for(false, terminal, desktop, false));
                if files {
                    let existing_fs = stored
                        .iter()
                        .find(|r| r.resource_type == nodeinnet_p2p::ResourceType::Filesystem)
                        .cloned();
                    resources.push(existing_fs.unwrap_or_else(app_net::default_fs_resource));
                }
                if network {
                    let existing_net = stored
                        .into_iter()
                        .find(|r| r.resource_type == nodeinnet_p2p::ResourceType::SharedNetwork);
                    resources.push(existing_net.unwrap_or_else(|| {
                        app_net::resources_for(false, false, false, true)
                            .pop()
                            .expect("resources_for(network) yields one")
                    }));
                }
                app_net::store_shared_services(&net.config, &resources);
                let _ = net
                    .net_tx
                    .try_send(app_net::NetCmd::ReloadResources(resources));
            }
        }
        ReloadSharedServices => {
            if let Some(net) = &app.net {
                let resources = app_net::default_shared_resources(&net.config);
                let _ = net
                    .net_tx
                    .try_send(app_net::NetCmd::ReloadResources(resources));
            }
        }
        ReportVisiblePage { name } => {
            app.visible_page = Some(name);
        }
        ReportRegistryView {
            path,
            subkeys_shown,
            values_shown,
            expanded,
        } => {
            app.registry_view = Some(json!({
                "path": path,
                "subkeys_shown": subkeys_shown,
                "values_shown": values_shown,
                "expanded": expanded,
            }));
        }
        ReportField { field, value } => {
            app.wizard_fields.insert(field, value);
        }
        AuthLogin {
            login,
            password,
            guest,
        } => {
            app.session.sign_in_as(login, password, guest).await;
        }
        AuthGuest => {
            app.session.sign_in_as_guest();
        }
        AuthLogout => {
            app.session.sign_out().await;
        }
        AuthRestore { reply } => {
            let ok = app.session.restore().await;
            let out = if ok {
                Ok(app.session.state().clone())
            } else {
                Err(app
                    .session
                    .state()
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "restore failed".into()))
            };
            let _ = reply.send(out);
        }

        DesktopConnect { connect, opts } => {
            if !connect {
                app.desktop.disconnect();
            } else if app.desktop.state().connected {
                app.desktop.set_stream_options(opts);
            } else {
                app.desktop.connect(opts);
            }
        }
        DesktopControl { enabled } => {
            app.desktop.set_control(enabled);
        }
        DesktopInput { event } => {
            app.desktop.input(event);
        }

        WebVpnStart => {
            app.webvpn.start();
        }
        WebVpnStop => {
            app.webvpn.stop();
            if let Some(peer) = app.workspace.selected_device().map(|d| d.id.clone()) {
                app.socks.close_peer(&peer);
            }
        }
        WebVpnAddApp { name, exec_cmd } => {
            if let Some(net) = &app.net {
                client_config::apps::upsert(
                    &net.config,
                    client_config::apps::ProxiedApp::new(name, exec_cmd, None),
                );
            }
            if let Some(tx) = &app.self_cmd {
                let _ = tx.send(ApiCmd::ProxiedAppsChanged);
            }
        }
        WebVpnRemoveApp { id } => {
            if let Some(net) = &app.net {
                client_config::apps::remove_by_id(&net.config, &id);
            }
            if let Some(tx) = &app.self_cmd {
                let _ = tx.send(ApiCmd::ProxiedAppsChanged);
            }
        }
        ProxiedAppsChanged => {}
        WebVpnLaunch { exec_cmd } => {
            let peer = app.workspace.selected_device().map(|d| d.id.clone());
            let resource = app.webvpn.state().resource_id;
            let net_tx = app.net.as_ref().map(|n| n.net_tx.clone());
            match (peer, resource, net_tx) {
                (Some(peer), Some(resource), Some(net_tx)) => {
                    if let Err(e) =
                        app.socks
                            .launch(&peer, &resource, &exec_cmd, net_tx, app.webvpn.router())
                    {
                        eprintln!("[net] ✗ could not launch {exec_cmd}: {e}");
                    }
                }
                _ => eprintln!("[net] ✗ no peer with shared network selected"),
            }
        }

        RegistryRequestKeys { path } => app.reg.request_keys(path).await,
        RegistryCreateKey {
            parent_path,
            key_name,
        } => app.reg.create_key(parent_path, key_name).await,
        RegistrySetValue {
            path,
            value_name,
            data,
        } => app.reg.set_value(path, value_name, data).await,
        RegistryDeleteEntry {
            path,
            value_name,
            is_key,
        } => app.reg.delete_entry(path, value_name, is_key).await,
        WebVpnFetch { method, url, reply } => {
            let _ = reply.send(
                app.webvpn
                    .fetch(&method, &url, Default::default(), None)
                    .await,
            );
        }

        NetAttach {
            net,
            api_target,
            ws_override,
        } => {
            let auth = app_net::ServerAuth::new(&net, api_target, ws_override);
            if let Some(cmd) = app.self_cmd.clone() {
                auth.set_registry_sink(move |nodes| {
                    let _ = cmd.send(ApiCmd::NetRegistry { nodes });
                });
            }
            app.session.set_rpc(std::rc::Rc::new(auth));
            app.socks = net.socks.clone();
            app.net = Some(*net);
        }
        NetNodes { nodes } => {
            let selected = app.workspace.selected_device().map(|d| d.id.clone());
            let was_online = selected
                .as_ref()
                .is_some_and(|id| app.nodes.iter().any(|n| &n.id == id && n.is_online));
            app.nodes = nodes;
            sync_devices(app);
            if let Some(id) = selected {
                let now_online = app.nodes.iter().any(|n| n.id == id && n.is_online);
                if now_online && !was_online {
                    wire_selected_device(app, &id);
                }
            }
        }
        NetPeerGone { peer } => app.socks.close_peer(&peer),
        NetWsState { connected } => {
            app.workspace.set_server_online(connected);
        }
        NetPeerState { peer, state } => {
            app.peer_links.insert(peer, state);
            sync_devices(app);
        }
        NetRegistry { nodes } => {
            app.registry = nodes;
            sync_devices(app);
        }

        QueryState { reply } => {
            let _ = reply.send(json!({
                "workspace": app.workspace.snapshot(),
                "active_panel": app.fm.active(),
                "clipboard": app.fm.clipboard(),
                "left": app.fm.panel(Side::Left),
                "right": app.fm.panel(Side::Right),
                "terminal": app.terminal.state(),
                "desktop": app.desktop.state(),
                "webvpn": app.webvpn.state(),
                "sysinfo": app.sysinfo.state(),
                "session": app.session.state(),
                "visible_page": app.visible_page,
                "registry_view": app.registry_view,
                "shared": app.net.as_ref().map(|n| {
                    app_net::shared_services(&n.config)
                        .into_iter()
                        .map(|k| k.id())
                        .collect::<Vec<_>>()
                }),
                "fields": app.wizard_fields,
            }));
        }
        QueryPanel { side, reply } => {
            let _ = reply.send(app.fm.panel(side).clone());
        }
        QuerySession { reply } => {
            let _ = reply.send(app.session.state().clone());
        }
        QueryRemoteApps { reply } => {
            let view = selected_peer_resource(app).and_then(|(_, res)| {
                app.net
                    .as_ref()
                    .and_then(|n| n.routers.remote_apps.get(&res))
            });
            let _ = reply.send(view);
        }
        RemoteAppsRefresh => {
            let Some((node, resource)) = selected_peer_node(app) else {
                return;
            };
            let Some(net) = &app.net else { return };
            let Some(rpc) = app_net::remote_apps_rpc(net, &node) else {
                return;
            };
            let cache = net.routers.remote_apps.clone();
            tokio::task::spawn_local(async move {
                match rpc.list().await {
                    Ok(view) => cache.put(&resource, view),
                    Err(_) => cache.put(
                        &resource,
                        app_core::remote_apps::RemoteAppsView {
                            refused: Some("not_supported".into()),
                            ..Default::default()
                        },
                    ),
                }
            });
        }
        RemoteAppLaunchThere { app_id } => {
            let Some((node, _)) = selected_peer_node(app) else {
                return;
            };
            let Some(net) = &app.net else { return };
            let Some(rpc) = app_net::remote_apps_rpc(net, &node) else {
                return;
            };
            let self_cmd = app.self_cmd.clone();
            tokio::task::spawn_local(async move {
                if let Err(e) = rpc.launch(&app_id, uuid::Uuid::new_v4()).await {
                    eprintln!("[net] ✗ the peer would not start {app_id}: {e}");
                }
                if let Some(tx) = self_cmd {
                    let _ = tx.send(ApiCmd::RemoteAppsRefresh);
                }
            });
        }
        RemoteAppStopThere { session_id } => {
            let Some((node, _)) = selected_peer_node(app) else {
                return;
            };
            let Some(net) = &app.net else { return };
            let Some(rpc) = app_net::remote_apps_rpc(net, &node) else {
                return;
            };
            let self_cmd = app.self_cmd.clone();
            tokio::task::spawn_local(async move {
                if let Err(e) = rpc.stop(session_id).await {
                    eprintln!("[net] ✗ the peer would not stop that session: {e}");
                }
                if let Some(tx) = self_cmd {
                    let _ = tx.send(ApiCmd::RemoteAppsRefresh);
                }
            });
        }
        QueryTunnels { reply } => {
            app.socks.reap();
            let _ = reply.send((app.socks.totals(), app.socks.apps()));
        }
        SetAppRemoteLaunch { id, allowed } => {
            let mut told: Vec<String> = Vec::new();
            if let Some(net) = &app.net {
                if !client_config::apps::set_remote_launch(&net.config, &id, allowed) {
                    eprintln!("[net] ✗ no application with id {id}");
                }
            }
            if !allowed {
                for session in app.socks.sessions_of_app(&id) {
                    app.socks
                        .terminate_session(&session.peer_id, session.session_id);
                    if !told.contains(&session.peer_id) {
                        told.push(session.peer_id);
                    }
                }
            }
            for peer in told {
                push_session_change(app, &peer, "consent_revoked");
            }
            if let Some(tx) = &app.self_cmd {
                let _ = tx.send(ApiCmd::ProxiedAppsChanged);
            }
        }
        TerminateApp { pid } => {
            if !app.socks.terminate(pid) {
                eprintln!("[net] ✗ no launched application with pid {pid}");
            }
        }
        CloseTunnels => app.socks.close_all(),
        ReapTick => {
            for gone in app.socks.reap() {
                if gone.remote {
                    push_session_change(app, &gone.peer_id, "exited");
                }
            }
        }
        QueryWebVpn { reply } => {
            let _ = reply.send(app.webvpn.state());
        }
        QueryRegistry { reply } => {
            let _ = reply.send(app.reg.state());
        }
        QueryProxiedApps { reply } => {
            let apps = app
                .net
                .as_ref()
                .map(|n| client_config::apps::load(&n.config))
                .unwrap_or_default();
            let _ = reply.send(
                apps.into_iter()
                    .filter_map(|a| serde_json::to_value(a).ok())
                    .collect(),
            );
        }
        QueryDeviceServices { id, reply } => {
            let services = app
                .nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| {
                    services_of(&n.resources)
                        .into_iter()
                        .map(|k| k.id().to_string())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();
            let _ = reply.send(services);
        }
        FmReadFile { side, path, reply } => {
            let result = match app.fm.active_provider(side) {
                Some(p) => p
                    .read_file_opt(path, None, false)
                    .await
                    .map_err(|e| e.to_string()),
                None => Err("this panel has no filesystem".to_string()),
            };
            let _ = reply.send(result);
        }
        FmWriteFile {
            side,
            path,
            content,
            reply,
        } => {
            let result = match app.fm.active_provider(side) {
                Some(p) => p
                    .write_file(path, content, None, None)
                    .await
                    .map_err(|e| e.to_string()),
                None => Err("this panel has no filesystem".to_string()),
            };
            let _ = reply.send(result);
        }
        FmListDir { side, path, reply } => {
            let result = match app.fm.active_provider(side) {
                Some(p) => p.list_dir(path).await.map_err(|e| e.to_string()),
                None => Err("this panel has no filesystem".to_string()),
            };
            let _ = reply.send(result);
        }
        FmProviderInfo { side, reply } => {
            let info = app
                .fm
                .active_provider(side)
                .map(|p| (p.is_local(), p.is_read_only()));
            let _ = reply.send(info);
        }
        QueryTerminalRouter { reply } => {
            let _ = reply.send(app.terminal.router());
        }
        QueryDesktopRouter { reply } => {
            let _ = reply.send(app.desktop.router());
        }
    }
}

fn wire_selected_device(app: &mut App, device_id: &str) {
    let Some(net) = &app.net else {
        return;
    };
    let Some(node) = app.nodes.iter().find(|n| n.id == device_id) else {
        return;
    };
    if !node.is_online {
        unwire_services(app);
        return;
    }
    app.fm
        .set_resources(Side::Right, app_net::fs_tabs(net, node));
    match app_net::first_resource(node, nodeinnet_p2p::ResourceType::Terminal) {
        Some(res) => app
            .terminal
            .set_resource(res, net.sender_to(node.id.clone())),
        None => app.terminal.unwire(),
    }
    match app_net::first_resource(node, nodeinnet_p2p::ResourceType::RemoteDesktop) {
        Some(res) => app.desktop.set_resource(
            res.clone(),
            net.sender_to(node.id.clone()),
            net.media_sender(node.id.clone(), res),
        ),
        None => app.desktop.unwire(),
    }
    match app_net::first_resource(node, nodeinnet_p2p::ResourceType::SharedNetwork) {
        Some(res) => app.webvpn.set_resource(res, net.sender_to(node.id.clone())),
        None => app.webvpn.unwire(),
    }
    match app_net::registry_rpc(net, node) {
        Some(rpc) => app.reg.wire(rpc),
        None => app.reg.unwire(),
    }
    match app_net::sysinfo_rpc(net, node) {
        Some(rpc) => app.sysinfo.wire(rpc),
        None => app.sysinfo.unwire(),
    }
}

fn unwire_services(app: &mut App) {
    app.fm.set_resources(Side::Right, Vec::new());
    app.terminal.unwire();
    app.desktop.unwire();
    app.webvpn.unwire();
    app.reg.unwire();
    app.sysinfo.unwire();
}

fn mount_side(app: &mut App, side: Side) {
    let (resource_id, drive_name) = {
        let panel = app.fm.panel(side);
        let Some(active) = panel.active_resource else {
            return;
        };
        let Some(tab) = panel.resources.get(active) else {
            return;
        };
        (tab.id.clone(), tab.label.clone())
    };
    let Some(peer_id) = app.workspace.selected_device().map(|d| d.id.clone()) else {
        return;
    };
    let sender = match &app.net {
        Some(net) => net.peer_p2p_sender(peer_id),
        None => return,
    };
    let Some(port) = web_davserver::mount_resource(resource_id.clone(), drive_name.clone(), sender)
    else {
        return;
    };
    let url = format!("dav://localhost:{port}/");
    app.fm.set_mounted(side, Some(url.clone()));
    app.workspace.add_mount(app_core::workspace::MountInfo {
        resource_id,
        name: drive_name,
        url,
        port,
    });
}

fn unmount_side(app: &mut App, side: Side) {
    let resource_id = {
        let panel = app.fm.panel(side);
        let Some(active) = panel.active_resource else {
            return;
        };
        let Some(tab) = panel.resources.get(active) else {
            return;
        };
        tab.id.clone()
    };
    web_davserver::unmount_resource(&resource_id);
    app.fm.set_mounted(side, None);
    app.workspace.remove_mount(&resource_id);
}

fn selected_peer_node(app: &App) -> Option<(nodeinnet_p2p::NodeInfo, String)> {
    let id = app.workspace.selected_device().map(|d| d.id.clone())?;
    let node = app.nodes.iter().find(|n| n.id == id)?.clone();
    let resource = node
        .resources
        .iter()
        .find(|r| r.is_active && r.resource_type == nodeinnet_p2p::ResourceType::SharedNetwork)?
        .id
        .clone();
    Some((node, resource))
}

fn selected_peer_resource(app: &App) -> Option<(String, String)> {
    selected_peer_node(app).map(|(node, res)| (node.id, res))
}

fn push_session_change(app: &App, peer_id: &str, event: &str) {
    let Some(net) = &app.net else { return };
    let Some(resource) = app_net::default_shared_resources(&net.config)
        .into_iter()
        .find(|r| r.resource_type == nodeinnet_p2p::ResourceType::SharedNetwork && r.is_active)
        .map(|r| r.id)
    else {
        return;
    };
    let apps = client_config::apps::consented(&net.config)
        .into_iter()
        .map(|a| nodeinnet_p2p::p2p::LaunchableApp {
            id: a.id,
            name: a.name,
            icon_name: None,
        })
        .collect();
    let msg = nodeinnet_p2p::P2pMessage::AppListResponse {
        resource_id: resource,
        request_id: None,
        apps,
        sessions: app.socks.sessions_for(peer_id),
        refused: None,
        event: Some(event.to_string()),
    };
    let _ = net
        .net_tx
        .try_send(app_net::NetCmd::SendToPeer(peer_id.to_string(), msg));
}

async fn core_loop(
    mut app: App,
    mut rx: mpsc::UnboundedReceiver<ApiCmd>,
    events: broadcast::Sender<String>,
) {
    while let Some(cmd) = rx.recv().await {
        if let ApiCmd::Screenshot { path } = &cmd {
            let _ = events.send(json!({ "event": "screenshot", "path": path }).to_string());
            continue;
        }
        if let ApiCmd::SetWizardField { field, value } = &cmd {
            let _ = events
                .send(json!({ "event": "set_field", "field": field, "value": value }).to_string());
            continue;
        }
        if let ApiCmd::SetWizardService { kind, on } = &cmd {
            let _ = events.send(
                json!({ "event": "set_service", "service": kind.id(), "on": on }).to_string(),
            );
            continue;
        }
        if let ApiCmd::WizardSubmit = &cmd {
            let _ = events.send(json!({ "event": "submit" }).to_string());
            continue;
        }
        if let ApiCmd::ProxiedAppsChanged = &cmd {
            let _ = events.send(json!({ "event": "proxied_apps_changed" }).to_string());
            continue;
        }
        apply(&mut app, cmd).await;
        for ev in app.workspace.take_events() {
            let _ = events.send(serde_json::to_string(&ev).expect("serializable event"));
        }
        for ev in app.fm.take_events() {
            let _ = events.send(serde_json::to_string(&ev).expect("serializable event"));
        }
        for ev in app.terminal.take_events() {
            let _ = events.send(serde_json::to_string(&ev).expect("serializable event"));
        }
        for ev in app.session.take_events() {
            let _ = events.send(serde_json::to_string(&ev).expect("serializable event"));
        }
        for ev in app.desktop.take_events() {
            let _ = events.send(serde_json::to_string(&ev).expect("serializable event"));
        }
        for ev in app.webvpn.take_events() {
            let _ = events.send(serde_json::to_string(&ev).expect("serializable event"));
        }
        for ev in app.reg.take_events() {
            let _ = events.send(serde_json::to_string(&ev).expect("serializable event"));
        }
        for ev in app.sysinfo.take_events() {
            let _ = events.send(serde_json::to_string(&ev).expect("serializable event"));
        }
    }
}

struct Shared {
    cmd: mpsc::UnboundedSender<ApiCmd>,
    events: broadcast::Sender<String>,
}

fn parse_side(s: &str) -> Option<Side> {
    match s {
        "left" => Some(Side::Left),
        "right" => Some(Side::Right),
        _ => None,
    }
}

fn parse_conflict(s: &str) -> Option<app_core::fm::OnConflict> {
    use app_core::fm::OnConflict;
    match s {
        "replace" => Some(OnConflict::Replace),
        "skip" => Some(OnConflict::Skip),
        "keep_both" => Some(OnConflict::KeepBoth),
        _ => None,
    }
}

fn parse_kind(s: &str) -> Option<ServiceKind> {
    ServiceKind::from_id(s)
}

#[derive(Deserialize, Default)]
struct Body {
    id: Option<String>,
    kind: Option<String>,
    mode: Option<String>,
    perms: Option<u32>,
    theme: Option<String>,
    guest: Option<bool>,
    path: Option<String>,
    name: Option<String>,
    new_name: Option<String>,
    index: Option<usize>,
    rows: Option<u16>,
    cols: Option<u16>,
    login: Option<String>,
    password: Option<String>,
    connect: Option<bool>,
    enabled: Option<bool>,
    event: Option<nodeinnet_p2p::DesktopInputEvent>,
    method: Option<String>,
    url: Option<String>,
    field: Option<String>,
    value: Option<String>,
    value_name: Option<String>,
    data: Option<nodeinnet_p2p::p2p::RegistryValueData>,
    is_key: Option<bool>,
    paths: Option<Vec<String>>,
    resolutions: Option<Vec<(String, String)>>,
    files: Option<bool>,
    terminal: Option<bool>,
    desktop: Option<bool>,
    network: Option<bool>,
}

fn body_of(bytes: &web::Bytes) -> Body {
    if bytes.is_empty() {
        Body::default()
    } else {
        serde_json::from_slice(bytes).unwrap_or_default()
    }
}

fn accepted(shared: &Shared, cmd: ApiCmd) -> HttpResponse {
    let _ = shared.cmd.send(cmd);
    HttpResponse::Ok().json(json!({ "ok": true }))
}

fn bad(msg: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(json!({ "ok": false, "error": msg }))
}

async fn post_workspace(
    shared: web::Data<Shared>,
    path: web::Path<(String, String)>,
    bytes: web::Bytes,
) -> HttpResponse {
    let (scope, action) = path.into_inner();
    let body = body_of(&bytes);
    let cmd = match (scope.as_str(), action.as_str()) {
        ("device", "select") => match body.id {
            Some(id) => ApiCmd::SelectDevice { id },
            None => return bad("missing id"),
        },
        ("device", "close_tab") => match body.id {
            Some(id) => ApiCmd::CloseTab { id },
            None => return bad("missing id"),
        },
        ("service", act) => {
            let Some(kind) = body.kind.as_deref().and_then(parse_kind) else {
                return bad("missing/unknown kind");
            };
            match act {
                "open" => ApiCmd::OpenService { kind },
                "focus" => ApiCmd::FocusService { kind },
                "close" => ApiCmd::CloseService { kind },
                _ => return bad("unknown service action"),
            }
        }
        ("map", "peek") => ApiCmd::MapPeek,
        ("map", "full") => ApiCmd::MapFull,
        ("map", "close") => ApiCmd::MapClose,
        ("shell", "rail") => match body.mode.as_deref() {
            Some("list") => ApiCmd::SetRailMode {
                mode: RailViewMode::List,
            },
            Some("icons") => ApiCmd::SetRailMode {
                mode: RailViewMode::Icons,
            },
            _ => return bad("mode must be list|icons"),
        },
        ("shell", "theme") => match body.theme.as_deref() {
            Some("light") => ApiCmd::SetTheme {
                theme: Theme::Light,
            },
            Some("dark") => ApiCmd::SetTheme { theme: Theme::Dark },
            _ => return bad("theme must be light|dark"),
        },
        ("sysinfo", "refresh") => ApiCmd::SysInfoRefresh,
        ("setup", "begin") => ApiCmd::SetupBegin,
        ("setup", "enter") => ApiCmd::EnterWorkspace,
        ("setup", "name") => match body.name {
            Some(name) => ApiCmd::SetupName { name },
            None => return bad("missing name"),
        },
        ("setup", "services") => ApiCmd::SetSharedServices {
            files: body.files.unwrap_or(true),
            terminal: body.terminal.unwrap_or(true),
            desktop: body.desktop.unwrap_or(true),
            network: body.network.unwrap_or(true),
        },
        ("setup", "service") => match body.kind.as_deref().and_then(parse_kind) {
            Some(kind) => ApiCmd::SetWizardService {
                kind,
                on: body.enabled.unwrap_or(true),
            },
            None => return bad("missing/unknown kind"),
        },
        ("setup", "local-network") => ApiCmd::SetAllowLocalNetwork {
            allowed: body.enabled.unwrap_or(false),
        },
        ("setup", "reload") => ApiCmd::ReloadSharedServices,
        ("auth", "login") => match (body.login, body.password) {
            (Some(login), Some(password)) => ApiCmd::AuthLogin {
                login,
                password,
                guest: body.guest.unwrap_or(false),
            },
            _ => return bad("missing login/password"),
        },
        ("auth", "guest") => ApiCmd::AuthGuest,
        ("auth", "logout") => ApiCmd::AuthLogout,
        _ => return bad("unknown action"),
    };
    accepted(&shared, cmd)
}

async fn get_auth_state(shared: web::Data<Shared>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QuerySession { reply: tx });
    match rx.await {
        Ok(s) => HttpResponse::Ok().json(s),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn get_auth_restore(shared: web::Data<Shared>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::AuthRestore { reply: tx });
    match rx.await {
        Ok(Ok(s)) => HttpResponse::Ok().json(s),
        Ok(Err(e)) => HttpResponse::Unauthorized().json(json!({ "ok": false, "error": e })),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn post_desktop(
    shared: web::Data<Shared>,
    path: web::Path<String>,
    bytes: web::Bytes,
) -> HttpResponse {
    let body = body_of(&bytes);
    let cmd = match path.into_inner().as_str() {
        "connect" => match body.connect {
            Some(connect) => ApiCmd::DesktopConnect {
                connect,
                opts: Default::default(),
            },
            None => return bad("missing connect"),
        },
        "control" => match body.enabled {
            Some(enabled) => ApiCmd::DesktopControl { enabled },
            None => return bad("missing enabled"),
        },
        "input" => match body.event {
            Some(event) => ApiCmd::DesktopInput { event },
            None => return bad("missing event"),
        },
        _ => return bad("unknown desktop action"),
    };
    accepted(&shared, cmd)
}

async fn post_webvpn(
    shared: web::Data<Shared>,
    path: web::Path<String>,
    bytes: web::Bytes,
) -> HttpResponse {
    let body = body_of(&bytes);
    match path.into_inner().as_str() {
        "start" => accepted(&shared, ApiCmd::WebVpnStart),
        "stop" => accepted(&shared, ApiCmd::WebVpnStop),
        "app" => match (body.name, body.path) {
            (Some(name), Some(exec_cmd)) => {
                accepted(&shared, ApiCmd::WebVpnAddApp { name, exec_cmd })
            }
            _ => bad("missing name or path"),
        },
        "app-remove" => match body.name {
            Some(name) => {
                let (tx, rx) = oneshot::channel();
                let _ = shared.cmd.send(ApiCmd::QueryProxiedApps { reply: tx });
                let Ok(apps) = rx.await else {
                    return HttpResponse::ServiceUnavailable().finish();
                };
                match apps
                    .iter()
                    .find(|a| a["name"] == json!(name))
                    .and_then(|a| a["id"].as_str())
                {
                    Some(id) => accepted(&shared, ApiCmd::WebVpnRemoveApp { id: id.to_string() }),
                    None => bad("no application with that name"),
                }
            }
            None => bad("missing name"),
        },
        "remote-refresh" => accepted(&shared, ApiCmd::RemoteAppsRefresh),
        "remote-launch" => match body.id {
            Some(app_id) => accepted(&shared, ApiCmd::RemoteAppLaunchThere { app_id }),
            None => bad("missing id"),
        },
        "remote-stop" => match body.id.and_then(|s| s.parse().ok()) {
            Some(session_id) => accepted(&shared, ApiCmd::RemoteAppStopThere { session_id }),
            None => bad("missing or malformed id (a session uuid)"),
        },
        "terminate" => match body.index {
            Some(pid) => accepted(&shared, ApiCmd::TerminateApp { pid: pid as u32 }),
            None => bad("missing index (the pid)"),
        },
        "launch" => match body.path {
            Some(exec_cmd) => accepted(&shared, ApiCmd::WebVpnLaunch { exec_cmd }),
            None => bad("missing path"),
        },
        "fetch" => {
            let Some(url) = body.url else {
                return bad("missing url");
            };
            let method = body.method.unwrap_or_else(|| "GET".into());
            let (tx, rx) = oneshot::channel();
            let _ = shared.cmd.send(ApiCmd::WebVpnFetch {
                method,
                url,
                reply: tx,
            });
            match rx.await {
                Ok(Ok(resp)) => HttpResponse::Ok().json(json!({
                    "ok": true,
                    "status": resp.status,
                    "headers": resp.headers,
                    "body": String::from_utf8_lossy(&resp.body),
                })),
                Ok(Err(e)) => HttpResponse::BadGateway().json(json!({ "ok": false, "error": e })),
                Err(_) => HttpResponse::ServiceUnavailable().finish(),
            }
        }
        _ => bad("unknown webvpn action"),
    }
}

async fn get_device_services(shared: web::Data<Shared>, path: web::Path<String>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryDeviceServices {
        id: path.into_inner(),
        reply: tx,
    });
    match rx.await {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn get_webvpn_stats(shared: web::Data<Shared>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryWebVpn { reply: tx });
    match rx.await {
        Ok(s) => HttpResponse::Ok().json(s),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn get_registry_state(shared: web::Data<Shared>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryRegistry { reply: tx });
    match rx.await {
        Ok(s) => HttpResponse::Ok().json(s),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn post_registry(
    shared: web::Data<Shared>,
    path: web::Path<String>,
    bytes: web::Bytes,
) -> HttpResponse {
    let body = body_of(&bytes);
    let Some(key_path) = body.path else {
        return bad("missing path");
    };
    match path.into_inner().as_str() {
        "keys" => accepted(&shared, ApiCmd::RegistryRequestKeys { path: key_path }),
        "create" => {
            let Some(key_name) = body.name else {
                return bad("missing name");
            };
            accepted(
                &shared,
                ApiCmd::RegistryCreateKey {
                    parent_path: key_path,
                    key_name,
                },
            )
        }
        "set" => {
            let (Some(value_name), Some(data)) = (body.value_name, body.data) else {
                return bad("missing value_name/data");
            };
            accepted(
                &shared,
                ApiCmd::RegistrySetValue {
                    path: key_path,
                    value_name,
                    data,
                },
            )
        }
        "delete" => {
            let is_key = body.is_key.unwrap_or(false);
            if !is_key && body.value_name.is_none() {
                return bad("deleting a value needs value_name; pass is_key to delete the key");
            }
            accepted(
                &shared,
                ApiCmd::RegistryDeleteEntry {
                    path: key_path,
                    value_name: body.value_name,
                    is_key,
                },
            )
        }
        _ => bad("unknown registry action"),
    }
}

async fn get_webvpn_remote(shared: web::Data<Shared>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryRemoteApps { reply: tx });
    match rx.await {
        Ok(Some(view)) => HttpResponse::Ok().json(view),
        Ok(None) => HttpResponse::Ok().json(json!({ "apps": [], "sessions": [], "refused": null })),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn get_webvpn_tunnels(shared: web::Data<Shared>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryTunnels { reply: tx });
    match rx.await {
        Ok((totals, apps)) => HttpResponse::Ok().json(json!({
            "totals": totals,
            "apps": apps,
        })),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn get_webvpn_apps(shared: web::Data<Shared>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryProxiedApps { reply: tx });
    match rx.await {
        Ok(apps) => HttpResponse::Ok().json(apps),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn desktop_router(shared: &Shared) -> Option<Arc<DesktopRouter>> {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryDesktopRouter { reply: tx });
    rx.await.ok()
}

async fn get_desktop_stats(shared: web::Data<Shared>) -> HttpResponse {
    match desktop_router(&shared).await {
        Some(router) => HttpResponse::Ok().json(router.stats()),
        None => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn get_desktop_frame(shared: web::Data<Shared>) -> HttpResponse {
    let Some(router) = desktop_router(&shared).await else {
        return HttpResponse::ServiceUnavailable().finish();
    };
    let Some((w, h, bgra)) = router.latest_frame() else {
        return HttpResponse::NotFound().json(json!({ "ok": false, "error": "no frame yet" }));
    };
    let mut rgba = bgra;
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = match enc.write_header() {
            Ok(wr) => wr,
            Err(e) => return bad(&format!("png: {e}")),
        };
        if let Err(e) = writer.write_image_data(&rgba) {
            return bad(&format!("png: {e}"));
        }
    }
    HttpResponse::Ok().content_type("image/png").body(out)
}

async fn post_panel(
    shared: web::Data<Shared>,
    path: web::Path<(String, String)>,
    bytes: web::Bytes,
) -> HttpResponse {
    let (side_s, action) = path.into_inner();
    let Some(side) = parse_side(&side_s) else {
        return bad("side must be left|right");
    };
    let body = body_of(&bytes);
    let cmd = match action.as_str() {
        "resource" => match body.index {
            Some(index) => ApiCmd::FmSelectResource { side, index },
            None => return bad("missing index"),
        },
        "navigate" => match body.path {
            Some(path) => ApiCmd::FmNavigate { side, path },
            None => return bad("missing path"),
        },
        "enter" => ApiCmd::FmEnter { side },
        "up" => ApiCmd::FmUp { side },
        "breadcrumb" => match body.index {
            Some(index) => ApiCmd::FmBreadcrumb { side, index },
            None => return bad("missing index"),
        },
        "back" => ApiCmd::FmBack { side },
        "forward" => ApiCmd::FmForward { side },
        "refresh" => ApiCmd::FmRefresh { side },
        "activate" => ApiCmd::FmActivate { side },
        "cursor" => match body.index {
            Some(index) => ApiCmd::FmCursor { side, index },
            None => return bad("missing index"),
        },
        "select" => match body.index {
            Some(index) => ApiCmd::FmToggleSelect { side, index },
            None => return bad("missing index"),
        },
        "mkdir" => match body.name {
            Some(name) => ApiCmd::FmMkdir { side, name },
            None => return bad("missing name"),
        },
        "delete" => ApiCmd::FmDelete { side },
        "rename" => match body.new_name {
            Some(new_name) => ApiCmd::FmRename { side, new_name },
            None => return bad("missing new_name"),
        },
        "copy" => ApiCmd::FmCopy { side },
        "move" => ApiCmd::FmMove { side },
        "clipboard_cut" => ApiCmd::FmClipboardSet {
            side,
            kind: app_core::fm::TransferKind::Move,
        },
        "clipboard_copy" => ApiCmd::FmClipboardSet {
            side,
            kind: app_core::fm::TransferKind::Copy,
        },
        "clipboard_clear" => ApiCmd::FmClipboardClear,
        "paste" => ApiCmd::FmTransferRun {
            dest: side,
            from: TransferOrigin::Clipboard,
            into: body.name,
            resolutions: body
                .resolutions
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(n, r)| parse_conflict(&r).map(|r| (n, r)))
                .collect(),
        },
        "duplicate" => match (body.path, body.new_name) {
            (Some(path), Some(new_name)) => ApiCmd::FmDuplicate {
                side,
                path,
                new_name,
            },
            _ => return bad("missing path or new_name"),
        },
        "upload" => match (body.path, body.paths) {
            (Some(dir), Some(paths)) => ApiCmd::FmUpload {
                side,
                dir,
                files: paths.into_iter().map(std::path::PathBuf::from).collect(),
            },
            _ => return bad("missing path or paths"),
        },
        "chmod" => match (body.path, body.perms) {
            (Some(path), Some(mode)) => ApiCmd::FmChmod { side, path, mode },
            _ => return bad("missing path or perms"),
        },
        "mount" => ApiCmd::FmMount { side },
        "unmount" => ApiCmd::FmUnmount { side },
        "local" => match body.path {
            Some(path) => ApiCmd::FmSetLocalResource { side, path },
            None => return bad("missing path"),
        },
        _ => return bad("unknown panel action"),
    };
    accepted(&shared, cmd)
}

async fn post_terminal(
    shared: web::Data<Shared>,
    path: web::Path<String>,
    bytes: web::Bytes,
) -> HttpResponse {
    let body = body_of(&bytes);
    let cmd = match path.into_inner().as_str() {
        "start" => ApiCmd::TerminalStart,
        "stop" => ApiCmd::TerminalStop,
        "resize" => match (body.rows, body.cols) {
            (Some(rows), Some(cols)) => ApiCmd::TerminalResize { rows, cols },
            _ => return bad("missing rows/cols"),
        },
        _ => return bad("unknown terminal action"),
    };
    accepted(&shared, cmd)
}

async fn post_screenshot(shared: web::Data<Shared>, bytes: web::Bytes) -> HttpResponse {
    match body_of(&bytes).path {
        Some(path) => accepted(&shared, ApiCmd::Screenshot { path }),
        None => bad("missing path"),
    }
}

async fn post_wizard(
    shared: web::Data<Shared>,
    path: web::Path<String>,
    bytes: web::Bytes,
) -> HttpResponse {
    let body = body_of(&bytes);
    match path.into_inner().as_str() {
        "field" => match (body.field, body.value) {
            (Some(field), Some(value)) => {
                accepted(&shared, ApiCmd::SetWizardField { field, value })
            }
            _ => bad("missing field/value"),
        },
        "submit" => accepted(&shared, ApiCmd::WizardSubmit),
        _ => bad("unknown wizard action"),
    }
}

async fn get_state(shared: web::Data<Shared>) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryState { reply: tx });
    match rx.await {
        Ok(v) => HttpResponse::Ok().json(v),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

async fn get_panel_state(shared: web::Data<Shared>, path: web::Path<String>) -> HttpResponse {
    let Some(side) = parse_side(&path.into_inner()) else {
        return bad("side must be left|right");
    };
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryPanel { side, reply: tx });
    match rx.await {
        Ok(p) => HttpResponse::Ok().json(p),
        Err(_) => HttpResponse::ServiceUnavailable().finish(),
    }
}

struct EventsWs {
    rx: Option<broadcast::Receiver<String>>,
}

impl Actor for EventsWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.add_stream(BroadcastStream::new(self.rx.take().expect("rx set once")));
    }
}

impl StreamHandler<Result<String, tokio_stream::wrappers::errors::BroadcastStreamRecvError>>
    for EventsWs
{
    fn handle(
        &mut self,
        item: Result<String, tokio_stream::wrappers::errors::BroadcastStreamRecvError>,
        ctx: &mut Self::Context,
    ) {
        if let Ok(text) = item {
            ctx.text(text);
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for EventsWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(p)) => ctx.pong(&p),
            Ok(ws::Message::Close(reason)) => ctx.close(reason),
            _ => {}
        }
    }
}

async fn events_ws(
    req: HttpRequest,
    stream: web::Payload,
    shared: web::Data<Shared>,
) -> Result<HttpResponse, actix_web::Error> {
    ws::start(
        EventsWs {
            rx: Some(shared.events.subscribe()),
        },
        &req,
        stream,
    )
}

struct TerminalWs {
    rx: Option<broadcast::Receiver<Vec<u8>>>,
    cmd: mpsc::UnboundedSender<ApiCmd>,
}

impl Actor for TerminalWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.add_stream(BroadcastStream::new(self.rx.take().expect("rx set once")));
    }
}

impl StreamHandler<Result<Vec<u8>, tokio_stream::wrappers::errors::BroadcastStreamRecvError>>
    for TerminalWs
{
    fn handle(
        &mut self,
        item: Result<Vec<u8>, tokio_stream::wrappers::errors::BroadcastStreamRecvError>,
        ctx: &mut Self::Context,
    ) {
        if let Ok(data) = item {
            ctx.binary(data);
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for TerminalWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Binary(b)) => {
                let _ = self.cmd.send(ApiCmd::TerminalInput { data: b.to_vec() });
            }
            Ok(ws::Message::Text(t)) => {
                let _ = self.cmd.send(ApiCmd::TerminalInput {
                    data: t.into_bytes().to_vec(),
                });
            }
            Ok(ws::Message::Ping(p)) => ctx.pong(&p),
            Ok(ws::Message::Close(reason)) => ctx.close(reason),
            _ => {}
        }
    }
}

async fn terminal_ws(
    req: HttpRequest,
    stream: web::Payload,
    shared: web::Data<Shared>,
) -> Result<HttpResponse, actix_web::Error> {
    let (tx, rx) = oneshot::channel();
    let _ = shared.cmd.send(ApiCmd::QueryTerminalRouter { reply: tx });
    let router = rx
        .await
        .map_err(|_| actix_web::error::ErrorServiceUnavailable("core loop gone"))?;
    ws::start(
        TerminalWs {
            rx: Some(router.subscribe()),
            cmd: shared.cmd.clone(),
        },
        &req,
        stream,
    )
}

pub struct Headless {
    pub port: u16,
    pub cmd: mpsc::UnboundedSender<ApiCmd>,
    pub events: broadcast::Sender<String>,
}

pub fn start<F>(build: F) -> Headless
where
    F: FnOnce() -> App + Send + 'static,
{
    start_on_port(0, build)
}

pub fn start_on_port<F>(port: u16, build: F) -> Headless
where
    F: FnOnce() -> App + Send + 'static,
{
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (ev_tx, _) = broadcast::channel(1024);

    {
        let events = ev_tx.clone();
        let self_cmd = cmd_tx.clone();
        std::thread::Builder::new()
            .name("app-core-loop".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("core runtime");
                tokio::task::LocalSet::new().block_on(&rt, async move {
                    let mut app = build();
                    app.self_cmd = Some(self_cmd.clone());
                    tokio::task::spawn_local(async move {
                        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
                        loop {
                            tick.tick().await;
                            if self_cmd.send(ApiCmd::ReapTick).is_err() {
                                break;
                            }
                        }
                    });
                    core_loop(app, cmd_rx, events).await;
                });
            })
            .expect("spawn core loop");
    }

    let (port_tx, port_rx) = std::sync::mpsc::channel();
    {
        let cmd = cmd_tx.clone();
        let events = ev_tx.clone();
        std::thread::Builder::new()
            .name("app-headless-http".into())
            .spawn(move || {
                actix_web::rt::System::new().block_on(async move {
                    let shared = web::Data::new(Shared { cmd, events });
                    let server = HttpServer::new(move || {
                        actix_web::App::new()
                            .app_data(shared.clone())
                            .route("/api/state", web::get().to(get_state))
                            .route("/api/screenshot", web::post().to(post_screenshot))
                            .route("/api/wizard/{action}", web::post().to(post_wizard))
                            .route("/api/events", web::get().to(events_ws))
                            .route("/api/auth/state", web::get().to(get_auth_state))
                            .route("/api/auth/restore", web::get().to(get_auth_restore))
                            .route(
                                "/api/device/{id}/services",
                                web::get().to(get_device_services),
                            )
                            .route("/api/terminal/ws", web::get().to(terminal_ws))
                            .route("/api/terminal/{action}", web::post().to(post_terminal))
                            .route("/api/desktop/stats", web::get().to(get_desktop_stats))
                            .route("/api/desktop/frame", web::get().to(get_desktop_frame))
                            .route("/api/desktop/{action}", web::post().to(post_desktop))
                            .route("/api/webvpn/stats", web::get().to(get_webvpn_stats))
                            .route("/api/webvpn/apps", web::get().to(get_webvpn_apps))
                            .route("/api/webvpn/tunnels", web::get().to(get_webvpn_tunnels))
                            .route("/api/webvpn/remote", web::get().to(get_webvpn_remote))
                            .route("/api/webvpn/{action}", web::post().to(post_webvpn))
                            .route("/api/registry/state", web::get().to(get_registry_state))
                            .route("/api/registry/{action}", web::post().to(post_registry))
                            .route("/api/panel/{side}/state", web::get().to(get_panel_state))
                            .route("/api/panel/{side}/{action}", web::post().to(post_panel))
                            .route("/api/{scope}/{action}", web::post().to(post_workspace))
                    })
                    .workers(1)
                    .bind(("127.0.0.1", port))
                    .expect("bind headless server");
                    let port = server.addrs()[0].port();
                    let server = server.run();
                    port_tx.send(port).expect("report port");
                    let _ = server.await;
                });
            })
            .expect("spawn http thread");
    }

    let port = port_rx.recv().expect("headless server failed to start");
    Headless {
        port,
        cmd: cmd_tx,
        events: ev_tx,
    }
}
