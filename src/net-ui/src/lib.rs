mod i18n;

pub mod net_view;
pub mod settings;

pub use net_view::{
    format_size, NetworkInit, NetworkInput, NetworkModel, NetworkOutput, ProxiedApp,
};
pub use settings::{NetSettingsInit, NetSettingsModel, NetSettingsOutput};

use std::sync::Once;

static RESOURCES_INIT: Once = Once::new();

pub fn init_resources() {
    RESOURCES_INIT.call_once(|| {
        gtk::gio::resources_register_include!("net.gresource")
            .expect("Failed to register net-ui resources");
    });
}
