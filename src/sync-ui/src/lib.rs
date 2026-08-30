mod i18n;

pub mod settings;
pub mod sync_view;

pub use settings::{SyncSettingsInit, SyncSettingsInput, SyncSettingsModel, SyncSettingsOutput};
pub use sync_view::{
    DiffItem, SyncFolderInit, SyncFolderInput, SyncFolderModel, SyncFolderOutput, SyncOp,
};

use std::sync::Once;

static RESOURCES_INIT: Once = Once::new();

pub fn init_resources() {
    RESOURCES_INIT.call_once(|| {
        gtk::gio::resources_register_include!("sync.gresource")
            .expect("Failed to register sync-ui resources");
    });
}
