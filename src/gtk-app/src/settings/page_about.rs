use adw::prelude::*;

use crate::settings::caps_label;

pub(crate) fn about(parent: &gtk::Window) -> gtk::Box {
    let version = gtk::Label::new(Some(&crate::i18n::trf(
        "settings.version",
        &[
            ("version", app_version::APP_VERSION),
            ("build", app_version::BUILD_TYPE),
        ],
    )));
    version.add_css_class("vp-label");

    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    col.set_halign(gtk::Align::Center);
    col.set_margin_top(12);
    col.set_margin_bottom(12);
    col.append(&caps_label(&crate::i18n::tr("settings.about")));
    col.append(&version);
    #[cfg(not(target_os = "android"))]
    col.append(&check_update(parent));
    #[cfg(target_os = "android")]
    let _ = parent;
    col
}

#[cfg(not(target_os = "android"))]
fn check_update(parent: &gtk::Window) -> gtk::Button {
    let check = gtk::Button::with_label(&crate::i18n::tr("settings.check_update"));
    check.add_css_class("suggested-action");
    check.set_halign(gtk::Align::Center);
    let parent = parent.clone();
    check.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        btn.set_label(&crate::i18n::tr("settings.checking_update"));
        let btn = btn.clone();
        crate::updater::manual_check(parent.clone(), move || {
            btn.set_label(&crate::i18n::tr("settings.check_update"));
            btn.set_sensitive(true);
        });
    });
    check
}
