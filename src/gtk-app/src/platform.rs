use p2p_handlers::Capabilities;

pub struct Hosting {
    pub capabilities: Capabilities,
    /// Lets peers start this machine's desktop apps through its network (LD_PRELOAD launcher).
    pub launch_apps: bool,
}

/// What differs between the desktop binary and the Android app.
pub trait Platform {
    fn app_id(&self) -> &'static str {
        crate::APP_ID
    }

    /// The phone layout: one pane at a time, navigation instead of side by side, touch.
    fn mobile(&self) -> bool;

    /// The local REST/WS API that drives the app in headless tests; `None` runs the core alone.
    fn rest_port(&self) -> Option<u16>;

    fn hosting(&self) -> Hosting;

    fn device_name(&self) -> Option<String>;

    /// Where the local file panel starts.
    fn storage_root(&self) -> String;

    /// Back on the first screen: hide the app and keep the node running.
    fn put_away(&self, window: &gtk::Window);

    /// Asks for whatever the OS requires before local files can be listed.
    fn ensure_storage_access(&self, _window: &gtk::Window) {}

    /// Opens an in-app browser whose traffic goes through the SOCKS proxy on this port.
    fn open_browser(&self, _window: &gtk::Window, _proxy_port: u16) -> bool {
        false
    }
}

pub struct Desktop {
    rest_port: u16,
    mobile: bool,
}

impl Desktop {
    /// `--headless-port N` fixes the REST port; `NODEINNET_MOBILE=1` previews the phone layout.
    pub fn from_env() -> Self {
        let mut args = std::env::args();
        let mut rest_port = 0;
        while let Some(arg) = args.next() {
            if arg == "--headless-port" {
                rest_port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            }
        }
        Self {
            rest_port,
            mobile: std::env::var_os("NODEINNET_MOBILE").is_some(),
        }
    }
}

impl Platform for Desktop {
    fn mobile(&self) -> bool {
        self.mobile
    }

    fn rest_port(&self) -> Option<u16> {
        Some(self.rest_port)
    }

    /// The phone layout preview hosts what a phone does.
    fn hosting(&self) -> Hosting {
        if self.mobile {
            return Hosting {
                capabilities: Capabilities::FILESYSTEM
                    | Capabilities::NETWORK
                    | Capabilities::SYSTEM_INFO,
                launch_apps: false,
            };
        }
        Hosting {
            capabilities: Capabilities::ALL,
            launch_apps: true,
        }
    }

    fn device_name(&self) -> Option<String> {
        hostname::get().ok().and_then(|s| s.into_string().ok())
    }

    fn storage_root(&self) -> String {
        gtk::glib::home_dir().to_string_lossy().into_owned()
    }

    fn put_away(&self, window: &gtk::Window) {
        use gtk::prelude::*;
        window.minimize();
    }
}

pub(crate) fn hostable(kind: app_core::workspace::ServiceKind) -> bool {
    use app_core::workspace::ServiceKind as K;
    let cap = match kind {
        K::Files => Capabilities::FILESYSTEM,
        K::Terminal => Capabilities::TERMINAL,
        K::Desktop => Capabilities::REMOTE_DESKTOP,
        K::Network => Capabilities::NETWORK,
        K::Registry => Capabilities::REGISTRY,
        K::SystemInfo => Capabilities::SYSTEM_INFO,
    };
    crate::platform().hosting().capabilities.contains(cap)
}
