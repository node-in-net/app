//! The node.in.net Android app: the GTK client (src/gtk-app) run by GTK's Android runtime.
#![cfg(target_os = "android")]

mod jni_glue;

use nodeinnet_gtk::{Capabilities, Hosting, Platform};
use std::rc::Rc;

const APP_ID: &str = "net.nodeinnet.gtk";

struct Android;

impl Platform for Android {
    fn app_id(&self) -> &'static str {
        APP_ID
    }

    fn mobile(&self) -> bool {
        true
    }

    fn rest_port(&self) -> Option<u16> {
        None
    }

    fn hosting(&self) -> Hosting {
        Hosting {
            capabilities: Capabilities::FILESYSTEM
                | Capabilities::NETWORK
                | Capabilities::SYSTEM_INFO,
            launch_apps: false,
        }
    }

    fn device_name(&self) -> Option<String> {
        std::env::var("NODEINNET_DEVICE_NAME").ok()
    }

    fn storage_root(&self) -> String {
        std::env::var("NODEINNET_STORAGE").unwrap_or_else(|_| "/storage/emulated/0".to_string())
    }

    fn put_away(&self, window: &gtk::Window) {
        jni_glue::put_away(window);
    }

    fn ensure_storage_access(&self, window: &gtk::Window) {
        jni_glue::ensure_all_files_access(window);
    }

    fn open_browser(&self, window: &gtk::Window, proxy_port: u16) -> bool {
        jni_glue::open_browser(window, proxy_port)
    }
}

#[no_mangle]
pub extern "C" fn main(
    _argc: std::ffi::c_int,
    _argv: *mut *mut std::ffi::c_char,
    _envp: *mut *mut std::ffi::c_char,
) -> std::ffi::c_int {
    nodeinnet_gtk::run(Rc::new(Android));
    0
}
