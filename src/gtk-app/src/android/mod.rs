//! Android: GTK's runtime loads libnodeinnet_gtk.so and runs its `main()` on the GTK thread.

mod jni_glue;

use crate::{Capabilities, Hosting, Platform, VpnRequest};
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

    fn app_vpn(&self) -> bool {
        true
    }

    fn start_vpn(&self, window: &gtk::Window, request: &VpnRequest) -> bool {
        jni_glue::start_vpn(window, request)
    }

    fn stop_vpn(&self, window: &gtk::Window) {
        jni_glue::stop_vpn(window);
    }

    fn launcher_apps(&self, window: &gtk::Window) -> Vec<(String, String)> {
        jni_glue::launcher_apps(window)
    }

    fn launcher_packages(&self, window: &gtk::Window) -> Vec<String> {
        jni_glue::launcher_packages(window)
    }

    fn app_icon(&self, window: &gtk::Window, package: &str, size: i32) -> Option<Vec<u8>> {
        jni_glue::app_icon(window, package, size)
    }
}

#[no_mangle]
pub extern "C" fn main(
    _argc: std::ffi::c_int,
    _argv: *mut *mut std::ffi::c_char,
    _envp: *mut *mut std::ffi::c_char,
) -> std::ffi::c_int {
    jni_glue::stderr_to_logcat();
    crate::run(Rc::new(Android));
    0
}
