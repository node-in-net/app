use adw::prelude::*;
use app_headless::ApiCmd;
use tokio::sync::mpsc::UnboundedSender;

use crate::settings::caps_label;

const THEMES: [&str; 3] = [
    "settings.theme_system",
    "settings.theme_light",
    "settings.theme_dark",
];

pub(crate) fn theme(cmd: &UnboundedSender<ApiCmd>, config: &client_config::AppConfig) -> gtk::Box {
    let names: Vec<String> = THEMES.iter().map(|k| crate::i18n::tr(k)).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let combo = gtk::DropDown::builder()
        .model(&gtk::StringList::new(&refs))
        .selected(config.get::<u32>("ui.theme_index").unwrap_or(0))
        .halign(gtk::Align::Center)
        .build();

    {
        let config = config.clone();
        let cmd = cmd.clone();
        combo.connect_selected_notify(move |combo| {
            let index = combo.selected();
            config.set("ui.theme_index", index);
            config.save();
            crate::apply_theme(index);
            let dark = adw::StyleManager::default().is_dark();
            let _ = cmd.send(ApiCmd::SetTheme {
                theme: if dark {
                    app_core::workspace::Theme::Dark
                } else {
                    app_core::workspace::Theme::Light
                },
            });
        });
    }

    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    col.set_halign(gtk::Align::Center);
    col.set_margin_top(12);
    col.set_margin_bottom(12);
    col.append(&caps_label(&crate::i18n::tr("settings.theme")));
    col.append(&combo);
    col
}

pub(crate) fn language(config: &client_config::AppConfig) -> gtk::Box {
    let codes: Vec<String> = nodeinnet_i18n::languages()
        .map(|(c, _)| c.to_string())
        .collect();
    let names: Vec<&str> = nodeinnet_i18n::languages().map(|(_, n)| n).collect();

    let current = nodeinnet_i18n::current_lang();
    let combo = gtk::DropDown::builder()
        .model(&gtk::StringList::new(&names))
        .selected(codes.iter().position(|c| c == current).unwrap_or(0) as u32)
        .halign(gtk::Align::Center)
        .build();

    let hint = gtk::Label::new(Some(&crate::i18n::tr("settings.language_hint")));
    hint.add_css_class("vp-label");
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    hint.set_max_width_chars(40);
    hint.set_visible(false);

    let restart = gtk::Button::with_label(&crate::i18n::tr("settings.language_restart"));
    restart.add_css_class("suggested-action");
    restart.set_halign(gtk::Align::Center);
    restart.set_visible(false);
    restart.connect_clicked(|_| nodeinnet_utils::app::restart_app());

    {
        let config = config.clone();
        let hint = hint.clone();
        let restart = restart.clone();
        combo.connect_selected_notify(move |combo| {
            let Some(code) = codes.get(combo.selected() as usize) else {
                return;
            };
            config.set("ui.language", code);
            config.save();
            nodeinnet_i18n::set_lang(code);
            hint.set_visible(true);
            restart.set_visible(true);
        });
    }

    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    col.set_halign(gtk::Align::Center);
    col.set_margin_top(12);
    col.set_margin_bottom(12);
    col.append(&caps_label(&crate::i18n::tr("settings.language")));
    col.append(&combo);
    col.append(&hint);
    col.append(&restart);
    col
}
