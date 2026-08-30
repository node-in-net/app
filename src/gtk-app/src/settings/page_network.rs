use adw::prelude::*;
use app_headless::ApiCmd;
use tokio::sync::mpsc::UnboundedSender;

use crate::settings::caps_label;

pub(crate) fn relay_region(
    cmd: &UnboundedSender<ApiCmd>,
    config: &client_config::AppConfig,
) -> gtk::Box {
    const REGIONS: [app_net::TurnRegion; 4] = [
        app_net::TurnRegion::Auto,
        app_net::TurnRegion::Eu,
        app_net::TurnRegion::Us,
        app_net::TurnRegion::Main,
    ];

    let model = gtk::StringList::new(&[
        &crate::i18n::tr("settings.relay_region_auto"),
        &crate::i18n::tr("settings.relay_region_eu"),
        &crate::i18n::tr("settings.relay_region_us"),
        &crate::i18n::tr("settings.relay_region_main"),
    ]);
    let current = config.turn_region();
    let combo = gtk::DropDown::builder()
        .model(&model)
        .selected(REGIONS.iter().position(|r| *r == current).unwrap_or(0) as u32)
        .halign(gtk::Align::Center)
        .build();
    {
        let config = config.clone();
        let cmd = cmd.clone();
        combo.connect_selected_notify(move |combo| {
            if let Some(region) = REGIONS.get(combo.selected() as usize) {
                config.set_turn_region(*region);
                let _ = cmd.send(ApiCmd::SetTurnRegion { region: *region });
            }
        });
    }

    let hint = gtk::Label::new(Some(&crate::i18n::tr("settings.relay_region_hint")));
    hint.add_css_class("vp-label");
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    hint.set_max_width_chars(40);

    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    col.set_halign(gtk::Align::Center);
    col.set_margin_top(12);
    col.set_margin_bottom(12);
    col.append(&caps_label(&crate::i18n::tr("settings.relay_region")));
    col.append(&combo);
    col.append(&hint);
    col
}

pub(crate) fn network_limits(config: &client_config::AppConfig) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    col.set_halign(gtk::Align::Center);
    col.set_margin_top(12);
    col.set_margin_bottom(12);
    col.append(&caps_label(&crate::i18n::tr("settings.network_limits")));

    let spin = |value: u32, max: f64, step: f64| {
        gtk::SpinButton::builder()
            .adjustment(&gtk::Adjustment::new(
                f64::from(value),
                0.0,
                max,
                step,
                step * 10.0,
                0.0,
            ))
            .numeric(true)
            .build()
    };

    let labelled = |text: &str, w: &gtk::SpinButton| {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_halign(gtk::Align::Center);
        let l = gtk::Label::new(Some(text));
        l.set_width_chars(22);
        l.set_xalign(1.0);
        row.append(&l);
        row.append(w);
        row
    };

    let peers = spin(config.get::<u32>("ui.max_peers").unwrap_or(0), 256.0, 1.0);
    {
        let config = config.clone();
        peers.connect_value_changed(move |s| {
            let value = s.value() as u32;
            config.set("ui.max_peers", value);
            config.save();
            client_core::limits::set_max_peers(value);
        });
    }
    col.append(&labelled(&crate::i18n::tr("settings.max_peers"), &peers));

    let bandwidth = spin(
        config.get::<u32>("ui.bandwidth_limit").unwrap_or(0),
        1_048_576.0,
        64.0,
    );
    {
        let config = config.clone();
        bandwidth.connect_value_changed(move |s| {
            let value = s.value() as u32;
            client_core::limits::set_bandwidth_limit_kbps(value);
            let effective = client_core::limits::bandwidth_limit_kbps();
            if effective != value {
                s.set_value(f64::from(effective));
            }
            config.set("ui.bandwidth_limit", effective);
            config.save();
        });
    }
    col.append(&labelled(
        &crate::i18n::tr("settings.bandwidth"),
        &bandwidth,
    ));

    let hint = gtk::Label::new(Some(&crate::i18n::tr("settings.network_limits_hint")));
    hint.add_css_class("vp-label");
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    hint.set_max_width_chars(40);
    col.append(&hint);
    col
}
