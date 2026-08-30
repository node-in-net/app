use adw::prelude::*;
use app_core::desktop::{DesktopRouter, DesktopState};
use app_core::fm::{PanelState, Side};
use app_core::terminal::TerminalRouter;
use app_core::workspace::{DeviceInfo, LinkState, MountInfo, ServiceKind, WorkspaceSnapshot};
use app_headless::ApiCmd;
use gtk_net_ui::{NetworkInit, NetworkInput, NetworkModel, NetworkOutput};
use gtk_rdesk_ui::{
    RemoteDesktopInit, RemoteDesktopInput, RemoteDesktopModel, RemoteDesktopOutput,
};
use gtk_registry_ui::{RegistryInit, RegistryInput, RegistryModel, RegistryOutput};
use gtk_sysinfo_ui::{SysInfoInit, SysInfoInput, SysInfoModel, SysInfoOutput};
use gtk_terminal_ui::{TerminalInit, TerminalInput, TerminalModel, TerminalOutput};
use relm4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use crate::files::FilesPane;
use crate::i18n;
use crate::icons;

fn select_device(cmd: &UnboundedSender<ApiCmd>, id: &str) {
    let _ = cmd.send(ApiCmd::SelectDevice { id: id.to_string() });
    let _ = cmd.send(ApiCmd::FmSelectResource {
        side: Side::Right,
        index: 0,
    });
}

const RAIL_MIN: i32 = 200;
const RAIL_MAX: i32 = 400;

const SERVICES: [(ServiceKind, &str, &str); 6] = [
    (ServiceKind::SystemInfo, "sysinfo", "services.sysinfo"),
    (ServiceKind::Files, "fileexplorer", "services.files"),
    (ServiceKind::Terminal, "unix-console", "services.terminal"),
    (ServiceKind::Desktop, "display", "services.desktop"),
    (ServiceKind::Network, "vpn", "services.network"),
    (ServiceKind::Registry, "registry", "services.registry"),
];

pub struct WorkspaceInit {
    pub cmd: UnboundedSender<ApiCmd>,
    pub config: client_config::AppConfig,
}

#[derive(Debug)]
pub enum WorkspaceInput {
    Render(Box<WorkspaceSnapshot>),
    ReloadApps,
    WebVpn(Box<app_core::webvpn::WebVpnState>),
    Tunnels(app_net::TunnelTotals, Vec<app_net::LaunchedApp>),
    Panel(Side, Box<PanelState>),
    Desktop(Box<DesktopState>),
    Registry {
        path: String,
        subkeys: Vec<String>,
        values: Vec<app_core::registry::RegistryValueInfo>,
    },
    SysInfo(Box<app_core::sysinfo::SysInfo>),
}

pub struct WorkspaceModel {
    cmd: UnboundedSender<ApiCmd>,
    config: client_config::AppConfig,
    account_label: gtk::Label,
    device_list: gtk::ListBox,
    device_ids: Rc<RefCell<Vec<String>>>,
    tunnels_box: gtk::Box,
    tunnels_label: gtk::Label,
    tunnels_list: gtk::ListBox,
    tunnel_pids: Rc<RefCell<Vec<u32>>>,
    mounts_box: gtk::Box,
    mounts_list: gtk::ListBox,
    mount_rows: Rc<RefCell<Vec<MountInfo>>>,
    online_pill: gtk::Label,
    search: gtk::SearchEntry,
    device_tabs: gtk::Box,
    tabs: gtk::Box,
    content: gtk::Stack,
    deps_banner: adw::Banner,
    terminal: relm4::Controller<TerminalModel>,
    terminal_live: Cell<bool>,
    network: relm4::Controller<NetworkModel>,
    files: FilesPane,
    desktop: relm4::Controller<RemoteDesktopModel>,
    desktop_resource: RefCell<Option<String>>,
    registry: relm4::Controller<RegistryModel>,
    registry_live: Cell<bool>,
    sysinfo: relm4::Controller<SysInfoModel>,
    sysinfo_polling: Cell<bool>,
    session: RefCell<Session>,
    page: Cell<&'static str>,
}

#[derive(Default)]
struct Session {
    device: Option<String>,
    connected: bool,
}

fn dot_class(dev: &DeviceInfo) -> &'static str {
    use app_core::workspace::LinkState as L;
    if !dev.online {
        return "off";
    }
    match dev.link {
        L::Connected => "on",
        L::Connecting => "busy",
        L::Failed => "failed",
        L::Idle => "idle",
    }
}

fn link_tooltip(dev: &DeviceInfo) -> String {
    use app_core::workspace::LinkState as L;
    if !dev.online {
        return i18n::tr("link.offline");
    }
    i18n::tr(match dev.link {
        L::Connected => "link.connected",
        L::Connecting => "link.connecting",
        L::Failed => "link.failed",
        L::Idle => "link.idle",
    })
}

#[cfg(target_os = "linux")]
fn capture_deps_missing(config: &client_config::AppConfig) -> bool {
    app_net::is_shared(config, ServiceKind::Desktop)
        && !node_functions::desktop::is_gstreamer_pipewire_available()
}

#[cfg(not(target_os = "linux"))]
fn capture_deps_missing(_config: &client_config::AppConfig) -> bool {
    false
}

fn device_row(dev: &DeviceInfo) -> gtk::ListBoxRow {
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 11);
    row_box.set_margin_top(7);
    row_box.set_margin_bottom(7);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);

    row_box.append(&icons::image(icons::os_icon_name(&dev.os), 32));

    let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    text.set_hexpand(true);
    let name_l = gtk::Label::new(Some(&dev.name));
    name_l.add_css_class("device-name");
    name_l.set_halign(gtk::Align::Start);
    name_l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let sub = gtk::Label::new(Some(&icons::os_pretty(&dev.os).to_lowercase()));
    sub.add_css_class("device-sub");
    sub.set_halign(gtk::Align::Start);
    text.append(&name_l);
    text.append(&sub);
    row_box.append(&text);

    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.add_css_class("dot");
    dot.add_css_class(dot_class(dev));
    dot.set_tooltip_text(Some(&link_tooltip(dev)));
    dot.set_valign(gtk::Align::Center);
    row_box.append(&dot);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&row_box));
    row
}

fn apply_filter(list: &gtk::ListBox, needle: &str) {
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        let visible = needle.is_empty()
            || row
                .child()
                .and_then(|c| c.first_child().and_then(|i| i.next_sibling()))
                .and_then(|text| text.first_child())
                .and_then(|l| l.downcast::<gtk::Label>().ok())
                .map(|l| l.text().to_lowercase().contains(needle))
                .unwrap_or(true);
        row.set_visible(visible);
        index += 1;
    }
}

fn offline_placeholder() -> gtk::Widget {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 12);
    b.set_valign(gtk::Align::Center);
    b.set_halign(gtk::Align::Center);
    b.append(&icons::image("disconnect", 72));
    let t = gtk::Label::new(Some(&i18n::tr("workspace.device_offline")));
    t.add_css_class("title-2");
    b.append(&t);
    let s = gtk::Label::new(Some(&i18n::tr("workspace.device_offline_hint")));
    s.add_css_class("dim-label");
    b.append(&s);
    b.upcast()
}

fn service_placeholder(icon: &str, label: &str) -> gtk::Widget {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 12);
    b.set_valign(gtk::Align::Center);
    b.set_halign(gtk::Align::Center);
    b.append(&icons::image(icon, 72));
    let t = gtk::Label::new(Some(&i18n::tr(label)));
    t.add_css_class("title-2");
    b.append(&t);
    let s = gtk::Label::new(Some(&i18n::tr("workspace.coming_next")));
    s.add_css_class("dim-label");
    b.append(&s);
    b.upcast()
}

impl WorkspaceModel {
    fn device_tab(&self, dev: &DeviceInfo, active: bool) -> gtk::Box {
        let tab = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        tab.add_css_class("linked");
        tab.add_css_class("device-tab");
        if active {
            tab.add_css_class("active");
        }

        let main = gtk::Button::new();
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        inner.append(&icons::image(icons::os_icon_name(&dev.os), 18));
        let name = gtk::Label::new(Some(&dev.name));
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(20);
        inner.append(&name);
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("dot");
        dot.add_css_class(dot_class(dev));
        dot.set_tooltip_text(Some(&link_tooltip(dev)));
        dot.set_valign(gtk::Align::Center);
        inner.append(&dot);
        main.set_child(Some(&inner));
        {
            let cmd = self.cmd.clone();
            let id = dev.id.clone();
            main.connect_clicked(move |_| {
                select_device(&cmd, &id);
            });
        }

        let close = gtk::Button::with_label("✕");
        close.add_css_class("device-tab-close");
        {
            let cmd = self.cmd.clone();
            let id = dev.id.clone();
            close.connect_clicked(move |_| {
                let _ = cmd.send(ApiCmd::CloseTab { id: id.clone() });
            });
        }

        tab.append(&main);
        tab.append(&close);
        tab
    }

    fn tunnel_row(&self, app: &app_net::LaunchedApp) -> gtk::ListBoxRow {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row_box.set_margin_top(5);
        row_box.set_margin_bottom(5);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);

        row_box.append(&icons::image("vpn", 20));

        let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        text.set_hexpand(true);
        let binary = app
            .command
            .split_whitespace()
            .next()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .unwrap_or(&app.command);
        let name_l = gtk::Label::new(Some(binary));
        name_l.add_css_class("device-name");
        name_l.set_halign(gtk::Align::Start);
        name_l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let sub = gtk::Label::new(Some(&i18n::trf(
            "workspace.proxied_row_sub",
            &[("pid", &app.pid.to_string())],
        )));
        sub.add_css_class("device-sub");
        sub.set_halign(gtk::Align::Start);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&name_l);
        text.append(&sub);
        row_box.append(&text);

        let stop = gtk::Button::from_icon_name("process-stop-symbolic");
        stop.add_css_class("flat");
        stop.set_valign(gtk::Align::Center);
        stop.set_tooltip_text(Some(&i18n::tr("workspace.stop_app")));
        {
            let cmd = self.cmd.clone();
            let pid = app.pid;
            stop.connect_clicked(move |_| {
                let _ = cmd.send(ApiCmd::TerminateApp { pid });
            });
        }
        row_box.append(&stop);

        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&row_box));
        row
    }

    fn mount_row(&self, m: &MountInfo) -> gtk::ListBoxRow {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row_box.set_margin_top(5);
        row_box.set_margin_bottom(5);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);

        let icon = gtk::Image::from_icon_name("drive-harddisk-symbolic");
        icon.set_pixel_size(20);
        row_box.append(&icon);

        let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        text.set_hexpand(true);
        let name_l = gtk::Label::new(Some(&m.name));
        name_l.add_css_class("device-name");
        name_l.set_halign(gtk::Align::Start);
        name_l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let sub = gtk::Label::new(Some(&m.url));
        sub.add_css_class("device-sub");
        sub.set_halign(gtk::Align::Start);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&name_l);
        text.append(&sub);
        row_box.append(&text);

        let eject = gtk::Button::from_icon_name("media-eject-symbolic");
        eject.add_css_class("flat");
        eject.set_valign(gtk::Align::Center);
        eject.set_tooltip_text(Some(&i18n::tr("workspace.unmount")));
        {
            let cmd = self.cmd.clone();
            let rid = m.resource_id.clone();
            eject.connect_clicked(move |_| {
                let _ = cmd.send(ApiCmd::UnmountResource {
                    resource_id: rid.clone(),
                });
            });
        }
        row_box.append(&eject);

        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&row_box));
        row
    }

    fn render(&self, snap: &WorkspaceSnapshot) {
        let login = self
            .config
            .get::<String>("app.account_login")
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| i18n::tr("workspace.guest"));
        self.account_label.set_text(&login);

        while let Some(row) = self.device_list.row_at_index(0) {
            self.device_list.remove(&row);
        }
        let mut ids = Vec::new();
        for dev in &snap.devices {
            self.device_list.append(&device_row(dev));
            ids.push(dev.id.clone());
        }
        self.device_ids.replace(ids);
        let online = snap.devices.iter().filter(|d| d.online).count();
        let any_online = online > 0;
        self.online_pill.set_text(&format!(
            "● {}",
            if any_online {
                online
            } else {
                snap.devices.len()
            }
        ));
        self.online_pill.remove_css_class("idle");
        if !any_online {
            self.online_pill.add_css_class("idle");
        }
        self.online_pill
            .set_tooltip_text(Some(&i18n::tr(if any_online {
                "link.online_count"
            } else {
                "link.total_count"
            })));
        self.deps_banner
            .set_revealed(capture_deps_missing(&self.config));
        apply_filter(&self.device_list, &self.search.text().to_lowercase());

        let selected = snap
            .selected
            .as_ref()
            .and_then(|id| snap.devices.iter().find(|d| &d.id == id));
        let selected_index = snap
            .selected
            .as_ref()
            .and_then(|id| self.device_ids.borrow().iter().position(|d| d == id));
        match selected_index.and_then(|i| self.device_list.row_at_index(i as i32)) {
            Some(row) => self.device_list.select_row(Some(&row)),
            None => self.device_list.select_row(gtk::ListBoxRow::NONE),
        }

        while let Some(row) = self.mounts_list.row_at_index(0) {
            self.mounts_list.remove(&row);
        }
        for m in &snap.mounts {
            self.mounts_list.append(&self.mount_row(m));
        }
        self.mount_rows.replace(snap.mounts.clone());
        self.mounts_box.set_visible(!snap.mounts.is_empty());

        while let Some(child) = self.device_tabs.first_child() {
            self.device_tabs.remove(&child);
        }
        for id in &snap.open_tabs {
            let Some(dev) = snap.devices.iter().find(|d| &d.id == id) else {
                continue;
            };
            let active = snap.selected.as_deref() == Some(id.as_str());
            self.device_tabs.append(&self.device_tab(dev, active));
        }
        if snap.open_tabs.is_empty() {
            let hint = gtk::Label::new(Some(&i18n::tr("workspace.pick_device")));
            hint.add_css_class("device-sub");
            self.device_tabs.append(&hint);
        }

        while let Some(child) = self.tabs.first_child() {
            self.tabs.remove(&child);
        }
        if let Some(dev) = selected {
            for (kind, icon, label) in SERVICES {
                if !dev.services.contains(&kind) {
                    continue;
                }
                let tab = gtk::Button::new();
                let inner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                inner.append(&icons::image(icon, 20));
                inner.append(&gtk::Label::new(Some(&i18n::tr(label))));
                tab.set_child(Some(&inner));
                tab.add_css_class("service-tab");
                tab.add_css_class("flat");
                if snap.panes.open.contains(&kind) {
                    tab.add_css_class("open");
                }
                if snap.panes.focused == Some(kind) {
                    tab.add_css_class("focused");
                }
                let cmd = self.cmd.clone();
                tab.connect_clicked(move |_| {
                    let _ = cmd.send(ApiCmd::OpenService { kind });
                });
                self.tabs.append(&tab);
            }
        }

        let session = Session {
            device: selected.map(|d| d.id.clone()),
            connected: selected
                .map(|d| d.link == LinkState::Connected)
                .unwrap_or(false),
        };
        let restarted = session.connected && !self.session.borrow().connected;
        if session.device != self.session.borrow().device || restarted {
            self.terminal_live.set(false);
            self.registry_live.set(false);
            let _ = self.terminal.sender().send(TerminalInput::Clear);
        }
        self.session.replace(session);

        let offline = selected.map(|d| !d.online).unwrap_or(false);
        let page = if offline {
            "offline"
        } else {
            snap.panes
                .focused
                .filter(|_| selected.is_some())
                .map(tab_page_name)
                .unwrap_or("empty")
        };
        self.content.set_visible_child_name(page);
        let opened = self.page.replace(page) != page;

        if page == "terminal" {
            if !self.terminal_live.get() {
                self.terminal_live.set(true);
                let _ = self.cmd.send(ApiCmd::TerminalStart);
            }
            if opened {
                let _ = self.terminal.sender().send(TerminalInput::GrabFocus);
            }
        }
        if page == "registry" && !self.registry_live.get() {
            self.registry_live.set(true);
            let _ = self.registry.sender().send(RegistryInput::Reset);
        }

        let want_polling = page == ServiceKind::SystemInfo.id() && selected.is_some();
        if want_polling != self.sysinfo_polling.get() {
            self.sysinfo_polling.set(want_polling);
            let s = self.sysinfo.sender();
            let _ = s.send(SysInfoInput::ToggleAutoUpdate(want_polling));
            if want_polling {
                let _ = s.send(SysInfoInput::RequestRefresh);
            }
        }

        let peer_name = selected.map(|d| d.name.clone()).unwrap_or_default();
        let has_peer = selected
            .map(|d| d.services.contains(&ServiceKind::Network))
            .unwrap_or(false);
        let connected = selected
            .map(|d| matches!(d.link, app_core::workspace::LinkState::Connected))
            .unwrap_or(false);
        let _ = self.network.sender().send(NetworkInput::SetContext {
            peer_name,
            has_peer,
            connected,
        });
    }

    fn apply_desktop(&self, state: &DesktopState) {
        if state.resource_id != *self.desktop_resource.borrow() {
            self.desktop_resource.replace(state.resource_id.clone());
            if let Some(rid) = &state.resource_id {
                let _ = self.desktop.sender().send(RemoteDesktopInput::SetSession {
                    resource_id: rid.clone(),
                    local: false,
                });
            }
        }
        let _ = self
            .desktop
            .sender()
            .send(RemoteDesktopInput::ConnectionChanged {
                connected: state.connected,
                remote_w: 0,
                remote_h: 0,
            });
        let _ = self
            .desktop
            .sender()
            .send(RemoteDesktopInput::ControlChanged(state.controlling));
    }
}

impl SimpleComponent for WorkspaceModel {
    type Init = WorkspaceInit;
    type Input = WorkspaceInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let cmd = init.cmd;
        let config = init.config;

        let rail = gtk::Box::new(gtk::Orientation::Vertical, 0);
        rail.add_css_class("rail");
        rail.set_width_request(RAIL_MIN);

        let account_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        account_bar.set_margin_top(10);
        account_bar.set_margin_start(14);
        account_bar.set_margin_end(8);
        account_bar.set_margin_bottom(2);
        let account_label = gtk::Label::new(None);
        account_label.set_hexpand(true);
        account_label.set_halign(gtk::Align::Start);
        account_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        account_label.add_css_class("heading");
        let avatar = crate::icons::image("user-account", 18);
        let gear = gtk::Button::new();
        gear.set_child(Some(&crate::icons::image("setting", 16)));
        gear.add_css_class("flat");
        gear.set_tooltip_text(Some(&i18n::tr("workspace.shared_services")));
        {
            let cmd = cmd.clone();
            let config = config.clone();
            gear.connect_clicked(move |btn| {
                let window = btn.root().and_downcast::<gtk::Window>();
                crate::settings::open_settings_dialog(window, &cmd, &config);
            });
        }
        let signout = gtk::Button::new();
        signout.set_child(Some(&crate::icons::image("logout", 16)));
        signout.add_css_class("flat");
        signout.set_tooltip_text(Some(&i18n::tr("workspace.sign_out")));
        {
            let cmd = cmd.clone();
            signout.connect_clicked(move |_| {
                let _ = cmd.send(ApiCmd::AuthLogout);
            });
        }
        account_bar.append(&avatar);
        account_bar.append(&account_label);
        account_bar.append(&gear);
        account_bar.append(&signout);
        rail.append(&account_bar);

        let rail_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        rail_head.set_margin_top(14);
        rail_head.set_margin_start(14);
        rail_head.set_margin_end(12);
        let rail_title = gtk::Label::new(Some(&i18n::tr("workspace.devices")));
        rail_title.add_css_class("heading");
        rail_title.set_hexpand(true);
        rail_title.set_halign(gtk::Align::Start);
        let online_pill = gtk::Label::new(None);
        online_pill.add_css_class("online-pill");
        rail_head.append(&rail_title);
        rail_head.append(&online_pill);
        rail.append(&rail_head);

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some(&i18n::tr("workspace.find_device")));
        search.set_margin_top(10);
        search.set_margin_start(10);
        search.set_margin_end(10);
        rail.append(&search);

        let device_list = gtk::ListBox::new();
        device_list.add_css_class("navigation-sidebar");
        device_list.set_margin_top(6);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&device_list)
            .build();
        rail.append(&scroll);

        let tunnels_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        tunnels_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let tunnels_head = gtk::Label::new(Some(&i18n::tr("workspace.proxied_apps")));
        tunnels_head.add_css_class("heading");
        tunnels_head.set_halign(gtk::Align::Start);
        tunnels_head.set_margin_top(10);
        tunnels_head.set_margin_start(14);
        tunnels_head.set_margin_bottom(2);
        tunnels_box.append(&tunnels_head);
        let tunnels_label = gtk::Label::new(None);
        tunnels_label.add_css_class("dim-label");
        tunnels_label.set_halign(gtk::Align::Start);
        tunnels_label.set_margin_start(14);
        tunnels_label.set_margin_bottom(6);
        tunnels_label.set_wrap(true);
        tunnels_box.append(&tunnels_label);
        let tunnels_list = gtk::ListBox::new();
        tunnels_list.add_css_class("navigation-sidebar");
        tunnels_box.append(&tunnels_list);
        tunnels_box.set_visible(false);
        rail.append(&tunnels_box);

        let mounts_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        mounts_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let mounts_head = gtk::Label::new(Some(&i18n::tr("workspace.mounted_disks")));
        mounts_head.add_css_class("heading");
        mounts_head.set_halign(gtk::Align::Start);
        mounts_head.set_margin_top(10);
        mounts_head.set_margin_start(14);
        mounts_head.set_margin_bottom(2);
        mounts_box.append(&mounts_head);
        let mounts_list = gtk::ListBox::new();
        mounts_list.add_css_class("navigation-sidebar");
        mounts_box.append(&mounts_list);
        mounts_box.set_visible(false);
        rail.append(&mounts_box);

        {
            let cmd = cmd.clone();
            let sender = sender.clone();
            gtk::glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
                let cmd = cmd.clone();
                let sender = sender.clone();
                crate::tokio_rt().spawn(async move {
                    let (tx, rx) = oneshot::channel();
                    if cmd.send(ApiCmd::QueryTunnels { reply: tx }).is_err() {
                        return;
                    }
                    if let Ok((totals, apps)) = rx.await {
                        sender.input(WorkspaceInput::Tunnels(totals, apps));
                    }
                });
                gtk::glib::ControlFlow::Continue
            });
        }

        let ws = gtk::Box::new(gtk::Orientation::Vertical, 0);
        ws.set_hexpand(true);

        let deps_banner = adw::Banner::builder()
            .title(i18n::tr("deps.gstreamer_missing"))
            .button_label(i18n::tr("deps.install"))
            .revealed(capture_deps_missing(&config))
            .build();
        ws.append(&deps_banner);
        {
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            deps_banner.connect_button_clicked(move |b| {
                b.set_button_label(None::<&str>);
                b.set_title(&i18n::tr("deps.installing"));
                let tx = tx.clone();
                crate::tokio_rt().spawn(async move {
                    let _ = tx.send(node_functions::desktop::run_gstreamer_installer().await);
                });
            });
            let banner = deps_banner.clone();
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                if let Ok(res) = rx.try_recv() {
                    match res {
                        Ok(()) => {
                            banner.set_title(&i18n::tr("deps.installed_restart"));
                            banner.set_button_label(Some(&i18n::tr("deps.restart")));
                            let handled = std::cell::Cell::new(false);
                            banner.connect_button_clicked(move |_| {
                                if !handled.replace(true) {
                                    nodeinnet_utils::app::restart_app();
                                }
                            });
                        }
                        Err(e) => {
                            banner.set_title(&e);
                            banner.set_button_label(Some(&i18n::tr("deps.install")));
                        }
                    }
                }
                gtk::glib::ControlFlow::Continue
            });
        }

        let device_tabs = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        device_tabs.set_margin_top(8);
        device_tabs.set_margin_bottom(8);
        device_tabs.set_margin_start(14);
        device_tabs.set_margin_end(14);
        ws.append(&device_tabs);
        ws.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let tabs = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        tabs.set_margin_top(8);
        tabs.set_margin_start(14);
        tabs.set_margin_end(14);
        tabs.set_margin_bottom(8);
        ws.append(&tabs);
        ws.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let terminal = build_terminal(cmd.clone(), config.clone());
        let network = build_network(cmd.clone(), config.clone());
        let files = FilesPane::new(cmd.clone(), config.clone());
        let desktop = build_desktop(cmd.clone());
        let registry = build_registry(cmd.clone());
        let sysinfo = build_sysinfo(cmd.clone());

        let content = gtk::Stack::new();
        content.set_vexpand(true);
        content.add_named(
            &service_placeholder("connect", &i18n::tr("workspace.select_device")),
            Some("empty"),
        );
        content.add_named(&offline_placeholder(), Some("offline"));
        for (kind, _icon, _label) in SERVICES {
            match kind {
                ServiceKind::Files => {
                    content.add_named(&files.widget, Some(tab_page_name(kind)));
                }
                ServiceKind::Terminal => {
                    content.add_named(terminal.widget(), Some(tab_page_name(kind)));
                }
                ServiceKind::Network => {
                    content.add_named(network.widget(), Some(tab_page_name(kind)));
                }
                ServiceKind::Desktop => {
                    content.add_named(desktop.widget(), Some(tab_page_name(kind)));
                }
                ServiceKind::Registry => {
                    content.add_named(registry.widget(), Some(tab_page_name(kind)));
                }
                ServiceKind::SystemInfo => {
                    content.add_named(sysinfo.widget(), Some(tab_page_name(kind)));
                }
            }
        }
        ws.append(&content);

        let home = gtk::glib::home_dir();
        let home_s = home.to_string_lossy().to_string();
        let root_s = home
            .ancestors()
            .last()
            .map(|r| r.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        let start = {
            let rel = home_s.strip_prefix(&root_s).unwrap_or(&home_s);
            format!(
                "/{}",
                rel.trim_start_matches(['/', '\\']).replace('\\', "/")
            )
        };
        let _ = cmd.send(ApiCmd::FmSetLocalResource {
            side: Side::Left,
            path: root_s,
        });
        if start != "/" {
            let _ = cmd.send(ApiCmd::FmNavigate {
                side: Side::Left,
                path: start,
            });
        }

        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(&rail));
        paned.set_end_child(Some(&ws));
        paned.set_resize_start_child(false);
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);
        paned.set_position(224);
        paned.connect_position_notify(|p| {
            let pos = p.position();
            let clamped = pos.clamp(RAIL_MIN, RAIL_MAX);
            if pos != clamped {
                p.set_position(clamped);
            }
        });
        paned.set_vexpand(true);
        root.append(&paned);

        let device_ids: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let mount_rows: Rc<RefCell<Vec<MountInfo>>> = Rc::new(RefCell::new(Vec::new()));

        {
            let rows = mount_rows.clone();
            mounts_list.connect_row_activated(move |_, row| {
                let index = row.index();
                if index >= 0 {
                    if let Some(m) = rows.borrow().get(index as usize) {
                        web_davserver::open_in_explorer(m.port);
                    }
                }
            });
        }

        {
            let cmd = cmd.clone();
            let device_ids = device_ids.clone();
            device_list.connect_row_activated(move |_, row| {
                let index = row.index();
                if index >= 0 {
                    if let Some(id) = device_ids.borrow().get(index as usize) {
                        select_device(&cmd, id);
                        let _ = cmd.send(ApiCmd::OpenService {
                            kind: ServiceKind::Files,
                        });
                    }
                }
            });
        }
        {
            let list = device_list.clone();
            search.connect_search_changed(move |entry| {
                apply_filter(&list, &entry.text().to_lowercase());
            });
        }

        let model = WorkspaceModel {
            cmd,
            config,
            account_label,
            device_list,
            device_ids,
            tunnels_box,
            tunnels_label,
            tunnels_list,
            tunnel_pids: Rc::new(RefCell::new(Vec::new())),
            mounts_box,
            mounts_list,
            mount_rows,
            online_pill,
            search,
            device_tabs,
            tabs,
            content,
            deps_banner,
            terminal,
            terminal_live: Cell::new(false),
            network,
            files,
            desktop,
            desktop_resource: RefCell::new(None),
            registry,
            registry_live: Cell::new(false),
            sysinfo,
            sysinfo_polling: Cell::new(false),
            session: RefCell::new(Session::default()),
            page: Cell::new("empty"),
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WorkspaceInput::Render(snap) => self.render(&snap),
            WorkspaceInput::ReloadApps => {
                let _ = self.network.sender().send(NetworkInput::ReloadApps);
            }
            WorkspaceInput::Tunnels(totals, apps) => {
                self.tunnels_box.set_visible(totals.apps > 0);
                if totals.apps == 0 {
                    self.tunnel_pids.borrow_mut().clear();
                    return;
                }
                self.tunnels_label.set_label(&i18n::trf(
                    "workspace.proxied_summary",
                    &[
                        ("apps", &totals.apps.to_string()),
                        ("peers", &totals.peers.to_string()),
                        ("sockets", &totals.sockets.to_string()),
                        (
                            "traffic",
                            &gtk_net_ui::format_size(totals.bytes_up + totals.bytes_down),
                        ),
                    ],
                ));

                let pids: Vec<u32> = apps.iter().map(|a| a.pid).collect();
                if *self.tunnel_pids.borrow() == pids {
                    return;
                }
                self.tunnel_pids.replace(pids);
                while let Some(row) = self.tunnels_list.row_at_index(0) {
                    self.tunnels_list.remove(&row);
                }
                for app in &apps {
                    self.tunnels_list.append(&self.tunnel_row(app));
                }
            }
            WorkspaceInput::WebVpn(state) => {
                let live = state
                    .totals
                    .streams_opened
                    .saturating_sub(state.totals.streams_closed);
                let net = self.network.sender();
                let _ = net.send(NetworkInput::Totals {
                    sockets: live as usize,
                    rx: state.totals.bytes_down,
                    tx: state.totals.bytes_up,
                });
            }
            WorkspaceInput::Panel(side, panel) => self.files.apply(side, *panel),
            WorkspaceInput::Desktop(state) => self.apply_desktop(&state),
            WorkspaceInput::Registry {
                path,
                subkeys,
                values,
            } => {
                let _ = self.registry.sender().send(RegistryInput::Entries {
                    path,
                    subkeys,
                    values,
                });
            }
            WorkspaceInput::SysInfo(info) => {
                let _ = self.sysinfo.sender().send(SysInfoInput::Update(info));
            }
        }
    }
}

fn tab_page_name(kind: ServiceKind) -> &'static str {
    kind.id()
}

fn build_terminal(
    cmd: UnboundedSender<ApiCmd>,
    config: client_config::AppConfig,
) -> relm4::Controller<TerminalModel> {
    let cmd_out = cmd.clone();
    let terminal = TerminalModel::builder()
        .launch(TerminalInit {
            show_toolbar: true,
            back_tooltip: Some(i18n::tr("workspace.back_to_services")),
            config,
        })
        .connect_receiver(move |_input, output| match output {
            TerminalOutput::Input(data) => {
                let _ = cmd_out.send(ApiCmd::TerminalInput { data });
            }
            TerminalOutput::Resize { rows, cols } => {
                let _ = cmd_out.send(ApiCmd::TerminalResize { rows, cols });
            }
            TerminalOutput::Restart => {
                let _ = cmd_out.send(ApiCmd::TerminalStart);
            }
            TerminalOutput::Back => {}
        });

    let (byte_tx, byte_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let term_sender = terminal.sender().clone();
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(10), move || {
        let mut batch = Vec::new();
        while let Ok(data) = byte_rx.try_recv() {
            batch.extend_from_slice(&data);
        }
        if !batch.is_empty() {
            let _ = term_sender.send(TerminalInput::Feed(batch));
        }
        gtk::glib::ControlFlow::Continue
    });
    crate::tokio_rt().spawn(async move {
        let (rtx, rrx) = oneshot::channel::<Arc<TerminalRouter>>();
        if cmd
            .send(ApiCmd::QueryTerminalRouter { reply: rtx })
            .is_err()
        {
            return;
        }
        let Ok(router) = rrx.await else { return };
        let mut brx = router.subscribe();
        loop {
            match brx.recv().await {
                Ok(data) => {
                    if byte_tx.send(data).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    terminal
}

fn build_network(
    cmd: UnboundedSender<ApiCmd>,
    config: client_config::AppConfig,
) -> relm4::Controller<NetworkModel> {
    let cmd_out = cmd.clone();
    let network = NetworkModel::builder()
        .launch(NetworkInit {
            show_toolbar: true,
            config,
        })
        .connect_receiver(move |_input, output| match output {
            NetworkOutput::Launch(exec_cmd) => {
                let _ = cmd_out.send(ApiCmd::WebVpnLaunch { exec_cmd });
            }
            NetworkOutput::AllowRemoteLaunch { id, allowed } => {
                let _ = cmd_out.send(ApiCmd::SetAppRemoteLaunch { id, allowed });
            }
            NetworkOutput::RefreshRemoteApps => {
                let _ = cmd_out.send(ApiCmd::RemoteAppsRefresh);
            }
            NetworkOutput::LaunchThere(app_id) => {
                let _ = cmd_out.send(ApiCmd::RemoteAppLaunchThere { app_id });
            }
            NetworkOutput::StopThere(session_id) => {
                let _ = cmd_out.send(ApiCmd::RemoteAppStopThere { session_id });
            }
            NetworkOutput::Back => {}
        });

    let net_sender = network.sender().clone();
    let last: Arc<std::sync::Mutex<Option<NetStats>>> = Arc::new(std::sync::Mutex::new(None));
    gtk::glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
        let cmd = cmd.clone();
        let net_sender = net_sender.clone();
        let last = last.clone();
        crate::tokio_rt().spawn(async move {
            let (tx, rx) = oneshot::channel();
            if cmd.send(ApiCmd::QueryWebVpn { reply: tx }).is_err() {
                return;
            }
            let Ok(state) = rx.await else { return };
            let live = state
                .totals
                .streams_opened
                .saturating_sub(state.totals.streams_closed);
            let now = (
                state.active,
                live,
                state.totals.bytes_down,
                state.totals.bytes_up,
            );
            let changed = {
                let mut guard = last.lock().expect("net stats");
                if guard.as_ref() == Some(&now) {
                    false
                } else {
                    *guard = Some(now);
                    true
                }
            };
            if changed {
                let _ = net_sender.send(NetworkInput::Totals {
                    sockets: live as usize,
                    rx: now.2,
                    tx: now.3,
                });
            }

            let (tx, rx) = oneshot::channel();
            if cmd.send(ApiCmd::QueryRemoteApps { reply: tx }).is_ok() {
                if let Ok(view) = rx.await {
                    let _ = net_sender.send(NetworkInput::RemoteApps(view));
                }
            }
        });
        gtk::glib::ControlFlow::Continue
    });

    network
}

type NetStats = (bool, u64, u64, u64);

fn build_desktop(cmd: UnboundedSender<ApiCmd>) -> relm4::Controller<RemoteDesktopModel> {
    let cmd_out = cmd.clone();
    let desktop = RemoteDesktopModel::builder()
        .launch(RemoteDesktopInit { show_toolbar: true })
        .connect_receiver(move |_input, output| match output {
            RemoteDesktopOutput::Connect {
                connect,
                original_size,
                bitrate_bps,
                force_select,
            } => {
                let _ = cmd_out.send(ApiCmd::DesktopConnect {
                    connect,
                    opts: app_core::desktop::StreamOptions {
                        original_size,
                        bitrate_bps,
                        force_select,
                    },
                });
            }
            RemoteDesktopOutput::SetControl(enabled) => {
                let _ = cmd_out.send(ApiCmd::DesktopControl { enabled });
            }
            RemoteDesktopOutput::InputEvent(event) => {
                let _ = cmd_out.send(ApiCmd::DesktopInput { event });
            }
            RemoteDesktopOutput::Back => {}
        });

    let (router_tx, router_rx) = std::sync::mpsc::channel::<Arc<DesktopRouter>>();
    crate::tokio_rt().spawn(async move {
        let (tx, rx) = oneshot::channel();
        if cmd.send(ApiCmd::QueryDesktopRouter { reply: tx }).is_err() {
            return;
        }
        if let Ok(router) = rx.await {
            let _ = router_tx.send(router);
        }
    });

    let desk_sender = desktop.sender().clone();
    let mut router: Option<Arc<DesktopRouter>> = None;
    let mut last_seq = 0u64;
    let mut last_screen: Option<(usize, usize)> = None;
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
        if router.is_none() {
            if let Ok(r) = router_rx.try_recv() {
                router = Some(r);
            }
        }
        if let Some(r) = &router {
            let screen = r.screen_size();
            if screen.is_some() && screen != last_screen {
                last_screen = screen;
                if let Some((width, height)) = screen {
                    let _ = desk_sender.send(RemoteDesktopInput::ScreenSize { width, height });
                }
            }
            let seq = r.frame_seq();
            if seq != last_seq {
                last_seq = seq;
                if let Some((w, h, bgra)) = r.latest_frame() {
                    let _ = desk_sender.send(RemoteDesktopInput::Frame {
                        width: w as usize,
                        height: h as usize,
                        bgra,
                        compressed_len: 0,
                    });
                }
            }
        }
        gtk::glib::ControlFlow::Continue
    });

    desktop
}

fn build_registry(cmd: UnboundedSender<ApiCmd>) -> relm4::Controller<RegistryModel> {
    RegistryModel::builder()
        .launch(RegistryInit { show_toolbar: true })
        .connect_receiver(move |_input, output| match output {
            RegistryOutput::RequestKeys { path } => {
                let _ = cmd.send(ApiCmd::RegistryRequestKeys { path });
            }
            RegistryOutput::Rendered {
                path,
                subkeys_shown,
                values_shown,
                expanded,
            } => {
                let _ = cmd.send(ApiCmd::ReportRegistryView {
                    path,
                    subkeys_shown,
                    values_shown,
                    expanded,
                });
            }
            RegistryOutput::SetValue {
                path,
                value_name,
                data,
            } => {
                let _ = cmd.send(ApiCmd::RegistrySetValue {
                    path,
                    value_name,
                    data,
                });
            }
            RegistryOutput::DeleteEntry {
                path,
                is_key,
                value_name,
            } => {
                let _ = cmd.send(ApiCmd::RegistryDeleteEntry {
                    path,
                    value_name,
                    is_key,
                });
            }
            RegistryOutput::Back => {}
        })
}

fn build_sysinfo(cmd: UnboundedSender<ApiCmd>) -> relm4::Controller<SysInfoModel> {
    SysInfoModel::builder()
        .launch(SysInfoInit {
            show_toolbar: false,
            auto_update: false,
            update_interval: std::time::Duration::from_secs(2),
        })
        .connect_receiver(move |_input, output| match output {
            SysInfoOutput::RequestInfo => {
                let _ = cmd.send(ApiCmd::SysInfoRefresh);
            }
            SysInfoOutput::Back => {}
        })
}
