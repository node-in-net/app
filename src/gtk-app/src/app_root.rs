use adw::prelude::*;
use app_core::session::{SessionState, Stage};
use app_core::workspace::WorkspaceSnapshot;
use app_headless::ApiCmd;
use gtk::glib;
use relm4::prelude::*;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::mpsc::UnboundedSender;

use crate::i18n;
use crate::wizard::{WizardInit, WizardInput, WizardModel};
use crate::workspace::{WorkspaceInit, WorkspaceInput, WorkspaceModel};

pub struct AppInit {
    pub cmd: UnboundedSender<ApiCmd>,
    pub events: broadcast::Sender<String>,
    pub config: client_config::AppConfig,
    pub ui_rx: mpsc::Receiver<crate::UiEvent>,
    pub restoring: bool,
}

#[derive(Debug)]
pub enum AppInput {
    Event(serde_json::Value),
    State(serde_json::Value),
    Panic(String),
}

pub struct AppModel {
    window: adw::ApplicationWindow,
    config: client_config::AppConfig,
    server_dot: gtk::Box,
    region_flag: gtk::Image,
    title: adw::WindowTitle,
    root_stack: gtk::Stack,
    wizard: Controller<WizardModel>,
    workspace: Controller<WorkspaceModel>,
    cmd: UnboundedSender<ApiCmd>,
}

enum GuiMsg {
    Event(serde_json::Value),
    State(serde_json::Value),
}

fn is_structural(event: &str) -> bool {
    matches!(
        event,
        "devices_changed"
            | "selection_changed"
            | "services_changed"
            | "tabs_changed"
            | "map_changed"
            | "rail_mode_changed"
            | "theme_changed"
            | "mounts_changed"
    )
}

impl AppModel {
    fn render_server_pill(&self, online: bool) {
        self.server_dot.remove_css_class("on");
        self.server_dot.remove_css_class("off");
        self.server_dot
            .add_css_class(if online { "on" } else { "off" });
        self.server_dot.set_tooltip_text(Some(&i18n::tr(if online {
            "server.online"
        } else {
            "server.offline"
        })));

        let region = self.config.turn_region();
        let icon = match region {
            app_net::TurnRegion::Eu => "flag-eu",
            app_net::TurnRegion::Us => "flag-us",
            app_net::TurnRegion::Auto => "radar",
            _ => "setting",
        };
        self.region_flag
            .set_resource(Some(&format!("/com/nodeinnet/gtk/{icon}.svg")));
        self.region_flag
            .set_tooltip_text(Some(&i18n::tr(match region {
                app_net::TurnRegion::Eu => "relay.region_eu",
                app_net::TurnRegion::Us => "relay.region_us",
                app_net::TurnRegion::Auto => "relay.region_auto",
                _ => "relay.region_main",
            })));
    }

    fn render_session(&mut self, state: &SessionState) {
        if matches!(state.stage, Stage::Ready) {
            let entering = self.root_stack.visible_child_name().as_deref() != Some("workspace");
            if entering {
                let _ = self.cmd.send(ApiCmd::ReportVisiblePage {
                    name: "ready".into(),
                });
            }
            self.root_stack.set_visible_child_name("workspace");
            self.title.set_title("Node.In.Net");
            if entering {
                if self.window.is_mapped() {
                    self.window.set_size_request(1040, 680);
                }
                self.window.set_default_size(1040, 680);
            }
        } else {
            self.root_stack.set_visible_child_name("wizard");
            self.title.set_title(&WizardModel::window_title(state));
            self.wizard
                .sender()
                .send(WizardInput::Render(Box::new(state.clone())))
                .ok();
        }
    }
}

impl SimpleComponent for AppModel {
    type Init = AppInit;
    type Input = AppInput;
    type Output = ();
    type Root = adw::ApplicationWindow;
    type Widgets = ();

    fn init_root() -> Self::Root {
        adw::ApplicationWindow::builder()
            .title("Node.In.Net")
            .default_width(560)
            .default_height(620)
            .build()
    }

    fn init(
        init: Self::Init,
        window: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        gtk::gio::resources_register_include!("nodeinnet.gresource")
            .expect("Failed to register GTK resources.");
        if let Some(display) = gtk::gdk::Display::default() {
            let provider = gtk::CssProvider::new();
            provider.load_from_string(include_str!("style.css"));
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        crate::apply_theme(init.config.get("ui.theme_index").unwrap_or(0));

        let wizard = WizardModel::builder()
            .launch(WizardInit {
                cmd: init.cmd.clone(),
                config: init.config.clone(),
            })
            .detach();
        let workspace = WorkspaceModel::builder()
            .launch(WorkspaceInit {
                cmd: init.cmd.clone(),
                config: init.config.clone(),
            })
            .detach();

        let root_stack = gtk::Stack::new();
        root_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        root_stack.add_named(wizard.widget(), Some("wizard"));
        root_stack.add_named(workspace.widget(), Some("workspace"));
        root_stack.set_visible_child_name(if init.restoring {
            "workspace"
        } else {
            "wizard"
        });

        let title = adw::WindowTitle::new("Node.In.Net", "");
        let version = gtk::Label::new(Some(&format!("v{}", app_version::APP_VERSION)));
        version.add_css_class("app-version");
        version.set_valign(gtk::Align::Center);
        version.set_tooltip_text(Some(&i18n::tr("app.version_installed")));

        let server_dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        server_dot.add_css_class("dot");
        server_dot.set_valign(gtk::Align::Center);
        let region_flag = crate::icons::image("radar", 14);
        region_flag.set_valign(gtk::Align::Center);
        let server_pill = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        server_pill.add_css_class("server-pill");
        server_pill.append(&server_dot);
        server_pill.append(&region_flag);

        let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        title_box.set_halign(gtk::Align::Center);
        title_box.append(&title);
        title_box.append(&version);
        title_box.append(&server_pill);
        let header = adw::HeaderBar::builder().title_widget(&title_box).build();
        if !cfg!(target_os = "macos") {
            header.pack_end(
                &gtk::Separator::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .margin_top(8)
                    .margin_bottom(8)
                    .build(),
            );
        }
        header.pack_end(&theme_button(&init.config, &init.cmd));
        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&header);
        toolbar_view.set_content(Some(&root_stack));
        window.set_content(Some(&toolbar_view));

        let cmd_close = init.cmd.clone();
        window.connect_close_request(move |win| {
            let (tx, rx) = std::sync::mpsc::channel();
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            if cmd_close
                .send(app_headless::ApiCmd::QueryTunnels { reply: reply_tx })
                .is_ok()
            {
                crate::tokio_rt().spawn(async move {
                    let _ = tx.send(reply_rx.await.unwrap_or_default());
                });
            }
            let (running, _) = rx
                .recv_timeout(std::time::Duration::from_millis(300))
                .unwrap_or_default();

            if running.apps == 0 {
                web_davserver::unmount_all();
                return glib::Propagation::Proceed;
            }

            let dialog = adw::AlertDialog::builder()
                .heading(i18n::tr("workspace.quit_with_apps_title"))
                .body(i18n::trf(
                    "workspace.quit_with_apps_body",
                    &[("count", &running.apps.to_string())],
                ))
                .build();
            dialog.add_response("cancel", &i18n::tr("workspace.quit_cancel"));
            dialog.add_response("quit", &i18n::tr("workspace.quit_and_close_apps"));
            dialog.set_response_appearance("quit", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("cancel"));

            let win_for_dialog = win.clone();
            let win_for_quit = win.clone();
            let cmd_quit = cmd_close.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "quit" {
                    let _ = cmd_quit.send(app_headless::ApiCmd::CloseTunnels);
                    web_davserver::unmount_all();
                    win_for_quit.destroy();
                }
            });
            dialog.present(Some(&win_for_dialog));
            glib::Propagation::Stop
        });

        let (gui_tx, gui_rx) = mpsc::channel::<GuiMsg>();
        let cmd_b = init.cmd.clone();
        let mut events = init.events.subscribe();
        crate::tokio_rt().spawn(async move {
            fetch_state(&cmd_b, &gui_tx).await;
            loop {
                match events.recv().await {
                    Ok(line) => {
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                            continue;
                        };
                        let structural = v["event"].as_str().map(is_structural).unwrap_or(false);
                        if gui_tx.send(GuiMsg::Event(v)).is_err() {
                            break;
                        }
                        if structural {
                            fetch_state(&cmd_b, &gui_tx).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        fetch_state(&cmd_b, &gui_tx).await
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        let sender_drain = sender.clone();
        let ui_log = std::env::var_os("NODEINNET_UI_LOG").is_some();
        let mut last_tick = std::time::Instant::now();
        let mut worst = Duration::ZERO;
        let mut drained: u64 = 0;
        let mut reported = std::time::Instant::now();
        glib::timeout_add_local(Duration::from_millis(10), move || {
            if ui_log {
                let gap = last_tick.elapsed();
                last_tick = std::time::Instant::now();
                if gap > worst {
                    worst = gap;
                }
            }
            while let Ok(msg) = gui_rx.try_recv() {
                drained += 1;
                match msg {
                    GuiMsg::Event(v) => sender_drain.input(AppInput::Event(v)),
                    GuiMsg::State(v) => sender_drain.input(AppInput::State(v)),
                }
            }
            if ui_log && reported.elapsed() >= Duration::from_secs(2) {
                eprintln!(
                    "[ui] drained {drained} msgs, worst tick gap {} ms (timer asks for 10)",
                    worst.as_millis()
                );
                drained = 0;
                worst = Duration::ZERO;
                reported = std::time::Instant::now();
            }
            glib::ControlFlow::Continue
        });

        let sender_panic = sender.clone();
        let ui_rx = init.ui_rx;
        glib::timeout_add_local(Duration::from_millis(50), move || {
            while let Ok(crate::UiEvent::Panic(m)) = ui_rx.try_recv() {
                sender_panic.input(AppInput::Panic(m));
            }
            glib::ControlFlow::Continue
        });

        crate::updater::auto_check(window.clone().upcast());

        let model = AppModel {
            cmd: init.cmd.clone(),
            config: init.config.clone(),
            server_dot,
            region_flag,
            window,
            title,
            root_stack,
            wizard,
            workspace,
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            AppInput::Event(v) => match v["event"].as_str() {
                Some("session_changed") => {
                    match serde_json::from_value::<SessionState>(v["session"].clone()) {
                        Ok(state) => self.render_session(&state),
                        Err(e) => eprintln!("[ui] ✗ session_changed did not parse: {e}"),
                    }
                }
                Some("set_field") => {
                    if let (Some(field), Some(value)) = (v["field"].as_str(), v["value"].as_str()) {
                        self.wizard
                            .sender()
                            .send(WizardInput::SetField {
                                field: field.to_string(),
                                value: value.to_string(),
                            })
                            .ok();
                    }
                }
                Some("set_service") => {
                    if let (Some(kind), Some(on)) = (
                        v["service"]
                            .as_str()
                            .and_then(app_core::workspace::ServiceKind::from_id),
                        v["on"].as_bool(),
                    ) {
                        self.wizard
                            .sender()
                            .send(WizardInput::SetService { kind, on })
                            .ok();
                    }
                }
                Some("submit") => {
                    self.wizard.sender().send(WizardInput::Submit).ok();
                }
                Some("screenshot") => {
                    if let Some(path) = v["path"].as_str() {
                        capture_window(&self.window, path);
                    }
                }
                Some("panel_updated") => {
                    if let (Ok(side), Ok(panel)) = (
                        serde_json::from_value::<app_core::fm::Side>(v["side"].clone()),
                        serde_json::from_value::<app_core::fm::PanelState>(v["panel"].clone()),
                    ) {
                        self.workspace
                            .sender()
                            .send(WorkspaceInput::Panel(side, Box::new(panel)))
                            .ok();
                    }
                }
                Some("webvpn_changed") => {
                    if let Ok(state) =
                        serde_json::from_value::<app_core::webvpn::WebVpnState>(v["webvpn"].clone())
                    {
                        self.workspace
                            .sender()
                            .send(WorkspaceInput::WebVpn(Box::new(state)))
                            .ok();
                    }
                }
                Some("proxied_apps_changed") => {
                    self.workspace
                        .sender()
                        .send(WorkspaceInput::ReloadApps)
                        .ok();
                }
                Some("desktop_changed") => {
                    if let Ok(state) = serde_json::from_value::<app_core::desktop::DesktopState>(
                        v["desktop"].clone(),
                    ) {
                        self.workspace
                            .sender()
                            .send(WorkspaceInput::Desktop(Box::new(state)))
                            .ok();
                    }
                }
                Some("system_info_changed") => {
                    match serde_json::from_value::<app_core::sysinfo::SysInfo>(v["info"].clone()) {
                        Ok(info) => {
                            self.workspace
                                .sender()
                                .send(WorkspaceInput::SysInfo(Box::new(info)))
                                .ok();
                        }
                        Err(e) => eprintln!("[ui] ✗ system info did not parse: {e}"),
                    }
                }
                Some("registry_changed") => {
                    let path = v["path"].as_str().unwrap_or("/").to_string();
                    let subkeys = serde_json::from_value::<Vec<String>>(v["subkeys"].clone())
                        .unwrap_or_default();
                    let values =
                        serde_json::from_value::<Vec<app_core::registry::RegistryValueInfo>>(
                            v["values"].clone(),
                        )
                        .unwrap_or_default();
                    self.workspace
                        .sender()
                        .send(WorkspaceInput::Registry {
                            path,
                            subkeys,
                            values,
                        })
                        .ok();
                }
                _ => {}
            },
            AppInput::State(v) => {
                match serde_json::from_value::<SessionState>(v["session"].clone()) {
                    Ok(state) => self.render_session(&state),
                    Err(e) => eprintln!(
                        "[ui] ✗ session state did not parse: {e} | raw={}",
                        v["session"]
                    ),
                }
                if let Ok(snap) =
                    serde_json::from_value::<WorkspaceSnapshot>(v["workspace"].clone())
                {
                    self.render_server_pill(snap.server_online);
                    self.workspace
                        .sender()
                        .send(WorkspaceInput::Render(Box::new(snap)))
                        .ok();
                }
                for (key, side) in [
                    ("left", app_core::fm::Side::Left),
                    ("right", app_core::fm::Side::Right),
                ] {
                    if let Ok(panel) =
                        serde_json::from_value::<app_core::fm::PanelState>(v[key].clone())
                    {
                        self.workspace
                            .sender()
                            .send(WorkspaceInput::Panel(side, Box::new(panel)))
                            .ok();
                    }
                }
            }
            AppInput::Panic(message) => {
                let dialog = adw::AlertDialog::new(
                    Some(&crate::i18n::tr("app.critical_error")),
                    Some(&message),
                );
                dialog.add_response("close", &crate::i18n::tr("app.close"));
                dialog.present(Some(&self.window));
            }
        }
    }
}

async fn fetch_state(cmd: &UnboundedSender<ApiCmd>, gui_tx: &mpsc::Sender<GuiMsg>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = cmd.send(ApiCmd::QueryState { reply: tx });
    if let Ok(v) = rx.await {
        let _ = gui_tx.send(GuiMsg::State(v));
    }
}

fn capture_window(window: &adw::ApplicationWindow, path: &str) {
    let w = window.width().max(1);
    let h = window.height().max(1);
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, w as f64, h as f64);
    let Some(node) = snapshot.to_node() else {
        eprintln!("screenshot: empty snapshot");
        return;
    };
    let Some(renderer) = window.native().and_then(|n| n.renderer()) else {
        eprintln!("screenshot: no renderer (window not realized?)");
        return;
    };
    let texture = renderer.render_texture(&node, None);
    let bytes = texture.save_to_png_bytes();
    if let Err(e) = std::fs::write(path, &bytes) {
        eprintln!("screenshot: write {path} failed: {e}");
    }
}

fn theme_button(config: &client_config::AppConfig, cmd: &UnboundedSender<ApiCmd>) -> gtk::Button {
    let icon_for = |dark: bool| {
        if dark {
            "/com/nodeinnet/gtk/afternoon.svg"
        } else {
            "/com/nodeinnet/gtk/night.svg"
        }
    };

    let img = gtk::Image::from_resource(icon_for(adw::StyleManager::default().is_dark()));
    img.set_pixel_size(20);

    {
        let img = img.clone();
        adw::StyleManager::default().connect_dark_notify(move |sm| {
            img.set_resource(Some(icon_for(sm.is_dark())));
        });
    }

    let btn = gtk::Button::builder()
        .child(&img)
        .tooltip_text(crate::i18n::tr("settings.theme"))
        .build();
    btn.add_css_class("flat");
    btn.set_cursor_from_name(Some("pointer"));

    let config = config.clone();
    let cmd = cmd.clone();
    btn.connect_clicked(move |_| {
        let sm = adw::StyleManager::default();
        let to_dark = !sm.is_dark();
        sm.set_color_scheme(if to_dark {
            adw::ColorScheme::ForceDark
        } else {
            adw::ColorScheme::ForceLight
        });
        config.set("ui.theme_index", if to_dark { 2u32 } else { 1u32 });
        config.save();
        let _ = cmd.send(ApiCmd::SetTheme {
            theme: if to_dark {
                app_core::workspace::Theme::Dark
            } else {
                app_core::workspace::Theme::Light
            },
        });
    });
    btn
}
