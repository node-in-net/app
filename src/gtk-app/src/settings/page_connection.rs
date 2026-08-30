use adw::prelude::*;
use client_core::limits::{self, Timeouts};

use crate::settings::caps_label;

fn stored(config: &client_config::AppConfig) -> Timeouts {
    let d = Timeouts::default();
    Timeouts {
        ping_interval_secs: config
            .get::<u64>("net.ping_interval_secs")
            .unwrap_or(d.ping_interval_secs),
        pong_timeout_ms: config
            .get::<u64>("net.pong_timeout_ms")
            .unwrap_or(d.pong_timeout_ms),
        ws_connect_secs: config
            .get::<u64>("net.ws_connect_secs")
            .unwrap_or(d.ws_connect_secs),
        ..d
    }
}

pub(crate) fn apply_stored(config: &client_config::AppConfig) {
    limits::set_timeouts(stored(config));
}

pub(crate) fn build(config: &client_config::AppConfig) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    col.set_halign(gtk::Align::Center);
    col.set_margin_top(12);
    col.set_margin_bottom(12);
    col.append(&caps_label(&crate::i18n::tr("settings.timeouts")));

    let row = |text: &str, w: &gtk::SpinButton| {
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        r.set_halign(gtk::Align::Center);
        let l = gtk::Label::new(Some(text));
        l.set_width_chars(24);
        l.set_xalign(1.0);
        r.append(&l);
        r.append(w);
        r
    };

    let spin = |value: u64, min: f64, max: f64, step: f64| {
        gtk::SpinButton::builder()
            .adjustment(&gtk::Adjustment::new(
                value as f64,
                min,
                max,
                step,
                step * 10.0,
                0.0,
            ))
            .numeric(true)
            .build()
    };

    let current = stored(config);
    let ping = spin(current.ping_interval_secs, 1.0, 3600.0, 1.0);
    let pong = spin(current.pong_timeout_ms / 1000, 1.0, 86_400.0, 5.0);
    let connect = spin(current.ws_connect_secs, 1.0, 3600.0, 1.0);

    let commit = {
        let config = config.clone();
        let ping = ping.clone();
        let pong = pong.clone();
        let connect = connect.clone();
        move || {
            limits::set_timeouts(Timeouts {
                ping_interval_secs: ping.value() as u64,
                pong_timeout_ms: (pong.value() as u64) * 1000,
                ws_connect_secs: connect.value() as u64,
                ..Timeouts::default()
            });
            let live = limits::timeouts();
            if pong.value() as u64 != live.pong_timeout_ms / 1000 {
                pong.set_value((live.pong_timeout_ms / 1000) as f64);
            }
            config.set("net.ping_interval_secs", live.ping_interval_secs);
            config.set("net.pong_timeout_ms", live.pong_timeout_ms);
            config.set("net.ws_connect_secs", live.ws_connect_secs);
            config.save();
        }
    };

    for s in [&ping, &pong, &connect] {
        let commit = commit.clone();
        s.connect_value_changed(move |_| commit());
    }

    col.append(&row(&crate::i18n::tr("settings.ping_interval"), &ping));
    col.append(&row(&crate::i18n::tr("settings.pong_timeout"), &pong));
    col.append(&row(&crate::i18n::tr("settings.ws_connect"), &connect));

    let hint = gtk::Label::new(Some(&crate::i18n::tr("settings.timeouts_hint")));
    hint.add_css_class("vp-label");
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    hint.set_max_width_chars(44);
    col.append(&hint);
    col
}
