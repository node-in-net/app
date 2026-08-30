#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_root;
mod bridge;
mod files;
mod i18n;
mod icons;
mod settings;
mod updater;
mod viewer;
mod wizard;
mod workspace;

use adw::prelude::*;
use relm4::RelmApp;
use std::rc::Rc;
use std::sync::mpsc;

pub const APP_ID: &str = "com.nodeinnet.app";

fn link_state(state: app_net::P2pPeerState) -> app_core::workspace::LinkState {
    use app_core::workspace::LinkState as L;
    match state {
        app_net::P2pPeerState::Connected => L::Connected,
        app_net::P2pPeerState::ConnectingTransport | app_net::P2pPeerState::Authenticating => {
            L::Connecting
        }
        app_net::P2pPeerState::Failed => L::Failed,
        app_net::P2pPeerState::Disconnected => L::Idle,
    }
}

pub enum UiEvent {
    Panic(String),
}

pub(crate) static UI_PANIC_TX: std::sync::OnceLock<std::sync::Mutex<mpsc::Sender<UiEvent>>> =
    std::sync::OnceLock::new();

pub fn tokio_rt() -> &'static tokio::runtime::Runtime {
    static TOKIO_RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    TOKIO_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to initialize global Tokio Runtime for GTK")
    })
}

pub fn apply_theme(index: u32) {
    let sm = adw::StyleManager::default();
    match index {
        1 => sm.set_color_scheme(adw::ColorScheme::ForceLight),
        2 => sm.set_color_scheme(adw::ColorScheme::ForceDark),
        _ => sm.set_color_scheme(adw::ColorScheme::Default),
    }
}

fn headless_port_arg() -> Option<u16> {
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg == "--headless-port" {
            return args.next().and_then(|p| p.parse().ok());
        }
    }
    None
}

fn main() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }

    let application = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let config = client_config::AppConfig::new("app");
    nodeinnet_i18n::set_lang(
        &config
            .get::<String>("ui.language")
            .unwrap_or_else(|| "en".to_string()),
    );

    let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>();
    let _ = UI_PANIC_TX.set(std::sync::Mutex::new(ui_tx));
    std::panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic payload"
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let panic_info = format!("Thread panic: '{msg}'\n{location}");
        eprintln!("🔥 CRITICAL PANIC:\n{panic_info}");
        if let Some(mutex) = UI_PANIC_TX.get() {
            if let Ok(tx) = mutex.lock() {
                let _ = tx.send(UiEvent::Panic(panic_info));
            }
        }
    }));

    let suggested_name = config.get::<String>("app-name").unwrap_or_else(|| {
        hostname::get()
            .ok()
            .and_then(|s| s.into_string().ok())
            .unwrap_or_else(|| i18n::tr("app.my_computer"))
    });
    let identity = app_net::Identity::load_or_create("app", &suggested_name);
    let node_id = identity.my_info.id.clone();
    let app_config = identity.config.clone();
    let routers = app_net::Routers::new();
    let mock = std::env::var("NODEINNET_MOCK").is_ok();

    let restoring = app_config
        .get::<String>("app.refresh_token")
        .filter(|t| !t.is_empty())
        .is_some();

    settings::apply_stored_limits(&app_config);

    let port = headless_port_arg().unwrap_or(0);
    let headless = {
        let routers = routers.clone();
        let suggested = suggested_name.clone();
        app_headless::start_on_port(port, move || {
            let mut core = app_headless::App::new();
            core.terminal = app_core::terminal::Terminal::with_router(routers.terminal.clone());
            core.desktop = app_core::desktop::Desktop::with_router(routers.desktop.clone());
            core.webvpn = app_core::webvpn::WebVpn::with_router(routers.webvpn.clone());
            core.setup_identity = Some((node_id, suggested));
            if mock {
                core.session.set_rpc(Rc::new(crate::bridge::AcceptAnyAuth));
                core.workspace.apply_snapshot(crate::bridge::mock_devices());
                core.workspace.take_events();
            }
            core
        })
    };
    println!("🕹  headless REST on 127.0.0.1:{}", headless.port);

    if !mock {
        let net = {
            let _rt = tokio_rt().enter();
            let cmd = headless.cmd.clone();
            app_net::Net::spawn(identity, routers, move |ev| match ev {
                app_net::NetEvent::Nodes(nodes) => {
                    let _ = cmd.send(app_headless::ApiCmd::NetNodes { nodes });
                }
                app_net::NetEvent::PeerState(peer, state) => {
                    let _ = cmd.send(app_headless::ApiCmd::NetPeerState {
                        peer,
                        state: link_state(state),
                    });
                }
                app_net::NetEvent::PeerGone(peer) => {
                    let _ = cmd.send(app_headless::ApiCmd::NetPeerGone { peer });
                }
                app_net::NetEvent::Ws(connected) => {
                    let _ = cmd.send(app_headless::ApiCmd::NetWsState { connected });
                }
            })
        };
        let api_target =
            std::env::var("NODEINNET_API").unwrap_or_else(|_| "https://node.in.net".to_string());
        let _ = headless.cmd.send(app_headless::ApiCmd::NetAttach {
            net: Box::new(net),
            api_target,
            ws_override: std::env::var("NODEINNET_WS").ok(),
        });
        if restoring {
            let (tx, _rx) = tokio::sync::oneshot::channel();
            let _ = headless
                .cmd
                .send(app_headless::ApiCmd::AuthRestore { reply: tx });
        }
    }

    let init = app_root::AppInit {
        cmd: headless.cmd.clone(),
        events: headless.events.clone(),
        config: app_config,
        ui_rx,
        restoring,
    };

    let exe_name = std::env::args()
        .next()
        .unwrap_or_else(|| "nodeinnet-gtk".to_string());
    RelmApp::from_app(application.upcast::<gtk::Application>())
        .with_args(vec![exe_name])
        .run::<app_root::AppModel>(init);
}
