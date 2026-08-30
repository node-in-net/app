mod i18n;

pub mod rdesk_view;
pub mod settings;

pub use rdesk_view::{
    RemoteDesktopInit, RemoteDesktopInput, RemoteDesktopModel, RemoteDesktopOutput,
};
pub use settings::{
    get_resource_icon_path, RdeskSettingsInit, RdeskSettingsInput, RdeskSettingsModel,
    RdeskSettingsOutput,
};

use std::sync::Once;

static RESOURCES_INIT: Once = Once::new();

pub fn init_resources() {
    RESOURCES_INIT.call_once(|| {
        gtk::gio::resources_register_include!("rdesk.gresource")
            .expect("Failed to register rdesk-ui resources");
    });
}
