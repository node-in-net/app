use adw::prelude::*;
use app_core::workspace::ServiceKind;
use app_headless::ApiCmd;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;

use crate::icons;
use crate::settings::caps_label;

pub(crate) struct ServiceConfig {
    pub(crate) widget: gtk::Box,
    pub(crate) switches: Vec<(ServiceKind, gtk::Switch)>,
}

pub(crate) fn service_config(
    cmd: &UnboundedSender<ApiCmd>,
    config: &client_config::AppConfig,
) -> ServiceConfig {
    let specs = [
        ("sysinfo", "services.sysinfo", ServiceKind::SystemInfo),
        ("fileexplorer", "services.files", ServiceKind::Files),
        ("unix-console", "services.terminal", ServiceKind::Terminal),
        ("display", "services.desktop", ServiceKind::Desktop),
        ("vpn", "services.network", ServiceKind::Network),
    ];

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_halign(gtk::Align::Center);
    let mut switches = Vec::new();

    let shares_rev = gtk::Revealer::new();
    shares_rev.set_child(Some(&shares_editor(cmd, config)));
    shares_rev.set_reveal_child(app_net::is_shared(config, ServiceKind::Files));

    for (icon, label, kind) in specs {
        let on_now = app_net::is_shared(config, kind);
        let img = icons::image(icon, 40);
        if !on_now {
            img.add_css_class("svc-off");
        }
        let sw = gtk::Switch::new();
        sw.set_active(on_now);
        sw.set_halign(gtk::Align::Center);

        let tile = gtk::Box::new(gtk::Orientation::Vertical, 6);
        tile.set_width_request(96);
        tile.append(&img);
        let lbl = gtk::Label::new(Some(&crate::i18n::tr(label)));
        lbl.add_css_class("vp-label");
        tile.append(&lbl);
        tile.append(&sw);
        row.append(&tile);
        switches.push((kind, sw.clone()));

        let cmd = cmd.clone();
        let config = config.clone();
        if matches!(kind, ServiceKind::Files) {
            let shares_rev = shares_rev.clone();
            sw.connect_active_notify(move |sw| {
                let on = sw.is_active();
                if on {
                    img.remove_css_class("svc-off");
                } else {
                    img.add_css_class("svc-off");
                    app_net::set_file_shares(&config, &[]);
                    let _ = cmd.send(ApiCmd::ReloadSharedServices);
                }
                shares_rev.set_reveal_child(on);
            });
        } else {
            sw.connect_active_notify(move |sw| {
                let on = sw.is_active();
                if on {
                    img.remove_css_class("svc-off");
                } else {
                    img.add_css_class("svc-off");
                }
                app_net::set_service(&config, kind, on);
                let _ = cmd.send(ApiCmd::ReloadSharedServices);
            });
        }
    }

    if cfg!(target_os = "windows") {
        let on_now = app_net::is_registry_shared(config);
        let img = icons::image("registry", 40);
        if !on_now {
            img.add_css_class("svc-off");
        }
        let sw = gtk::Switch::new();
        sw.set_active(on_now);
        sw.set_halign(gtk::Align::Center);
        let tile = gtk::Box::new(gtk::Orientation::Vertical, 6);
        tile.set_width_request(96);
        tile.append(&img);
        let lbl = gtk::Label::new(Some(&crate::i18n::tr("services.registry")));
        lbl.add_css_class("vp-label");
        tile.append(&lbl);
        tile.append(&sw);
        row.append(&tile);
        switches.push((ServiceKind::Registry, sw.clone()));

        let cmd = cmd.clone();
        let config = config.clone();
        sw.connect_active_notify(move |sw| {
            let on = sw.is_active();
            if on {
                img.remove_css_class("svc-off");
            } else {
                img.add_css_class("svc-off");
            }
            app_net::set_registry_shared(&config, on);
            let _ = cmd.send(ApiCmd::ReloadSharedServices);
        });
    }

    let col = gtk::Box::new(gtk::Orientation::Vertical, 10);
    col.set_halign(gtk::Align::Center);
    col.set_margin_top(12);
    col.set_margin_bottom(12);
    col.append(&caps_label(&crate::i18n::tr("settings.what_to_share")));
    col.append(&row);
    col.append(&shares_rev);
    ServiceConfig {
        widget: col,
        switches,
    }
}

fn shares_editor(cmd: &UnboundedSender<ApiCmd>, config: &client_config::AppConfig) -> gtk::Box {
    let initial = app_net::current_shares(config);
    let shares: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(initial.clone()));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
    for (name, path) in initial {
        append_share_row(&list, &shares, cmd, config, name, path);
    }

    let add = gtk::Button::with_label(&crate::i18n::tr("settings.add_folder"));
    add.add_css_class("flat");
    add.set_halign(gtk::Align::Center);
    {
        let shares = shares.clone();
        let cmd = cmd.clone();
        let config = config.clone();
        let list = list.clone();
        add.connect_clicked(move |btn| {
            let window = btn.root().and_downcast::<gtk::Window>();
            let shares = shares.clone();
            let cmd = cmd.clone();
            let config = config.clone();
            let list = list.clone();
            gtk_dialogs::select_folder(
                window.as_ref(),
                &crate::i18n::tr("settings.choose_folder"),
                move |path| {
                    let path_s = path.to_string_lossy().to_string();
                    let base = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "share".into());
                    let mut name = base.clone();
                    let mut n = 2;
                    while shares.borrow().iter().any(|(nm, _)| nm == &name) {
                        name = format!("{base} {n}");
                        n += 1;
                    }
                    shares.borrow_mut().push((name.clone(), path_s.clone()));
                    append_share_row(&list, &shares, &cmd, &config, name, path_s);
                    app_net::set_file_shares(&config, shares.borrow().as_slice());
                    let _ = cmd.send(ApiCmd::ReloadSharedServices);
                },
            );
        });
    }

    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    col.set_halign(gtk::Align::Center);
    col.set_margin_top(4);
    col.append(&caps_label(&crate::i18n::tr("settings.shared_folders")));
    col.append(&list);
    col.append(&add);
    col
}

fn append_share_row(
    list: &gtk::Box,
    shares: &Rc<RefCell<Vec<(String, String)>>>,
    cmd: &UnboundedSender<ApiCmd>,
    config: &client_config::AppConfig,
    name: String,
    path: String,
) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let lbl = gtk::Label::new(Some(&format!("{name}  —  {path}")));
    lbl.set_hexpand(true);
    lbl.set_halign(gtk::Align::Start);
    lbl.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    lbl.set_max_width_chars(40);
    let rm = gtk::Button::from_icon_name("user-trash-symbolic");
    rm.add_css_class("flat");
    {
        let shares = shares.clone();
        let cmd = cmd.clone();
        let config = config.clone();
        let list = list.clone();
        let row = row.clone();
        rm.connect_clicked(move |_| {
            shares.borrow_mut().retain(|(nm, _)| nm != &name);
            list.remove(&row);
            app_net::set_file_shares(&config, shares.borrow().as_slice());
            let _ = cmd.send(ApiCmd::ReloadSharedServices);
        });
    }
    row.append(&lbl);
    row.append(&rm);
    list.append(&row);
}
