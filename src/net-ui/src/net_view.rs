#![allow(deprecated)]
use adw::prelude::*;
use relm4::prelude::*;
use std::fs;

pub use client_config::apps::ProxiedApp;

fn load_proxied_apps(config: &client_config::AppConfig) -> Vec<ProxiedApp> {
    let apps = client_config::apps::load(config);
    if !apps.is_empty() {
        return apps;
    }
    let mut path = gtk::glib::user_data_dir();
    path.push("nodeinnet");
    path.push("proxied_apps.json");
    if path.exists() {
        return client_config::apps::import_legacy(config, &path);
    }
    Vec::new()
}

pub fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

pub struct NetworkInit {
    pub show_toolbar: bool,
    pub config: client_config::AppConfig,
}

#[derive(Debug)]
pub enum NetworkInput {
    SetContext {
        peer_name: String,
        has_peer: bool,
        connected: bool,
    },
    Totals {
        sockets: usize,
        rx: u64,
        tx: u64,
    },
    ReloadApps,
}

#[derive(Debug)]
pub enum NetworkOutput {
    Launch(String),
    Back,
}

pub struct NetworkModel {
    show_toolbar: bool,
    config: client_config::AppConfig,
    peer_name: String,
    has_peer: bool,
    connected: bool,
    sockets: usize,
    rx: u64,
    tx: u64,
    apps: Vec<ProxiedApp>,
    apps_generation: u64,
}

pub struct NetworkWidgets {
    header: adw::HeaderBar,
    sockets_lbl: gtk::Label,
    traffic_lbl: gtk::Label,
    apps_list: gtk::ListBox,
    stack: gtk::Stack,
    add_btn_toolbar: gtk::Button,
    rendered_generation: u64,
    title: gtk::Label,
    hint: gtk::Label,
    launch_btns: Vec<gtk::Button>,
}

impl SimpleComponent for NetworkModel {
    type Init = NetworkInit;
    type Input = NetworkInput;
    type Output = NetworkOutput;
    type Root = gtk::Box;
    type Widgets = NetworkWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        crate::init_resources();
        let config = init.config;

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content_box.set_margin_top(24);
        content_box.set_margin_bottom(24);
        content_box.set_margin_start(24);
        content_box.set_margin_end(24);
        content_box.set_vexpand(true);

        let empty_title = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .title_widget(&empty_title)
            // Inside the phone's navigation view it would grow a second back button.
            .show_back_button(false)
            .build();

        let btn_back = gtk::Button::builder()
            .child(&gtk::Image::from_resource("/com/net-ui/gtk/arrow-left.svg"))
            .tooltip_text(&*crate::i18n::tr("net.back_to_services"))
            .build();
        btn_back.set_cursor_from_name(Some("pointer"));
        let sender_back = sender.clone();
        btn_back.connect_clicked(move |_| {
            let _ = sender_back.output(NetworkOutput::Back);
        });
        // On the phone the page's own back arrow leads out.
        btn_back.set_visible(!gtk_dialogs::touch());
        header.pack_start(&btn_back);

        header.pack_start(
            &gtk::Separator::builder()
                .orientation(gtk::Orientation::Vertical)
                .margin_top(8)
                .margin_bottom(8)
                .build(),
        );

        let add_btn_toolbar = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(&*crate::i18n::tr("net.add_application_tooltip"))
            .build();
        add_btn_toolbar.set_cursor_from_name(Some("pointer"));
        header.pack_start(&add_btn_toolbar);

        let active_sockets_lbl = gtk::Label::builder()
            .label(&*crate::i18n::trf(
                "net.sockets_count",
                &[("count", &*(0).to_string())],
            ))
            .margin_start(12)
            .margin_end(12)
            .build();
        let traffic_lbl = gtk::Label::builder()
            .label(&*crate::i18n::trf(
                "net.traffic_format",
                &[("rx", &*("0 B").to_string()), ("tx", &*("0 B").to_string())],
            ))
            .margin_start(12)
            .margin_end(12)
            .build();

        header.pack_end(&traffic_lbl);
        header.pack_end(&active_sockets_lbl);
        header.set_visible(init.show_toolbar);
        root.append(&header);

        let search_bar = gtk::SearchEntry::builder()
            .margin_bottom(12)
            .hexpand(true)
            .build();

        let apps_list = gtk::ListBox::builder()
            .css_classes(vec!["boxed-list"])
            .selection_mode(gtk::SelectionMode::None)
            .valign(gtk::Align::Start)
            .build();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .min_content_height(400)
            .vexpand(true)
            .child(&apps_list)
            .build();

        let list_vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
        list_vbox.append(&search_bar);
        list_vbox.append(&scrolled);

        let empty_status = adw::StatusPage::builder()
            .title(&*crate::i18n::tr("net.no_applications_title"))
            .description(&*crate::i18n::tr("net.no_applications_desc"))
            .icon_name("network-wired-symbolic")
            .vexpand(true)
            .build();

        let add_btn_empty = gtk::Button::builder()
            .label(&*crate::i18n::tr("net.add_application_btn"))
            .css_classes(vec!["suggested-action", "pill"])
            .halign(gtk::Align::Center)
            .margin_top(16)
            .build();
        add_btn_empty.set_cursor_from_name(Some("pointer"));
        empty_status.set_child(Some(&add_btn_empty));

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        stack.add_named(&empty_status, Some("empty"));
        stack.add_named(&list_vbox, Some("list"));

        let search_bar_clone = search_bar.clone();
        apps_list.set_filter_func(move |row| {
            if let Some(ar) = row.downcast_ref::<adw::ActionRow>() {
                let query = search_bar_clone.text().to_string().to_lowercase();
                if query.is_empty() {
                    return true;
                }
                let title = ar.title().to_string().to_lowercase();
                let sub = ar.subtitle().unwrap_or_default().to_string().to_lowercase();
                return title.contains(&query) || sub.contains(&query);
            }
            true
        });
        let lb_clone = apps_list.clone();
        search_bar.connect_search_changed(move |_| {
            lb_clone.invalidate_filter();
        });

        for btn in [&add_btn_toolbar, &add_btn_empty] {
            let sender_add = sender.clone();
            let config_add = config.clone();
            btn.connect_clicked(move |b| {
                if let Some(window) = b.root().and_downcast::<gtk::Window>() {
                    show_app_selector_dialog(&window, config_add.clone(), sender_add.clone());
                }
            });
        }

        let title = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .css_classes(vec!["title-4"])
            .build();
        let hint = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .wrap(true)
            .xalign(0.0)
            .margin_bottom(12)
            .css_classes(vec!["dim-label"])
            .build();
        content_box.append(&title);
        content_box.append(&hint);
        content_box.append(&stack);
        root.append(
            &adw::Clamp::builder()
                .maximum_size(720)
                .vexpand(true)
                .child(&content_box)
                .build(),
        );

        let model = NetworkModel {
            show_toolbar: init.show_toolbar,
            apps: load_proxied_apps(&config),
            config,
            peer_name: String::new(),
            has_peer: false,
            connected: false,
            sockets: 0,
            rx: 0,
            tx: 0,
            apps_generation: 1,
        };
        let widgets = NetworkWidgets {
            header,
            sockets_lbl: active_sockets_lbl,
            traffic_lbl,
            apps_list,
            stack,
            add_btn_toolbar,
            rendered_generation: 0,
            title,
            hint,
            launch_btns: Vec::new(),
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            NetworkInput::SetContext {
                peer_name,
                has_peer,
                connected,
            } => {
                self.peer_name = peer_name;
                self.has_peer = has_peer;
                self.connected = connected;
            }
            NetworkInput::Totals { sockets, rx, tx } => {
                self.sockets = sockets;
                self.rx = rx;
                self.tx = tx;
            }
            NetworkInput::ReloadApps => {
                self.apps = load_proxied_apps(&self.config);
                self.apps_generation += 1;
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, sender: ComponentSender<Self>) {
        widgets.header.set_visible(self.show_toolbar);

        widgets.sockets_lbl.set_label(&crate::i18n::trf(
            "net.sockets_count",
            &[("count", &*(self.sockets).to_string())],
        ));
        widgets.traffic_lbl.set_label(&crate::i18n::trf(
            "net.traffic_format",
            &[
                ("rx", &*(format_size(self.rx)).to_string()),
                ("tx", &*(format_size(self.tx)).to_string()),
            ],
        ));

        widgets.title.set_text(&crate::i18n::trf(
            "net.apps_title",
            &[("peer", &self.peer_name)],
        ));
        widgets.hint.set_text(&crate::i18n::trf(
            "net.launch_hint",
            &[("peer", &self.peer_name)],
        ));

        if widgets.rendered_generation != self.apps_generation {
            widgets.rendered_generation = self.apps_generation;
            rebuild_apps_list(self, widgets, &sender);
        }
        let reachable = self.has_peer && self.connected;
        for btn in &widgets.launch_btns {
            btn.set_sensitive(reachable);
        }
    }
}

fn rebuild_apps_list(
    model: &NetworkModel,
    widgets: &mut NetworkWidgets,
    sender: &ComponentSender<NetworkModel>,
) {
    let lst = &widgets.apps_list;
    while let Some(child) = lst.first_child() {
        lst.remove(&child);
    }
    widgets.launch_btns.clear();

    if model.apps.is_empty() {
        widgets.stack.set_visible_child_name("empty");
        widgets.add_btn_toolbar.set_visible(false);
    } else {
        widgets.stack.set_visible_child_name("list");
        widgets.add_btn_toolbar.set_visible(true);
    }

    for app in model.apps.iter() {
        let row = adw::ActionRow::builder()
            .title(&app.name)
            .subtitle(&app.exec_cmd)
            .build();

        let icon_img = match app.icon_name.as_deref() {
            Some(name) => {
                let img = gtk::Image::from_icon_name(name);
                img.add_css_class("icon-dropshadow");
                img
            }
            None => gtk::Image::from_resource("/com/net-ui/gtk/vpn.svg"),
        };
        icon_img.set_pixel_size(32);
        row.add_prefix(&icon_img);

        let launch_btn = gtk::Button::builder()
            .label(&*crate::i18n::tr("net.launch_btn"))
            .css_classes(vec!["suggested-action"])
            .valign(gtk::Align::Center)
            .sensitive(model.has_peer && model.connected)
            .build();
        launch_btn.set_cursor_from_name(Some("pointer"));
        row.add_suffix(&launch_btn);
        widgets.launch_btns.push(launch_btn.clone());

        let remove_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(vec!["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text(&*crate::i18n::tr("net.remove_btn_confirm"))
            .build();
        remove_btn.set_cursor_from_name(Some("pointer"));
        row.add_suffix(&remove_btn);

        let exec_cmd = app.exec_cmd.clone();
        let sender_launch = sender.clone();
        launch_btn.connect_clicked(move |_| {
            let _ = sender_launch.output(NetworkOutput::Launch(exec_cmd.clone()));
        });

        let sender_rm = sender.clone();
        let config_rm = model.config.clone();
        let id_rm = app.id.clone();
        remove_btn.connect_clicked(move |btn| {
            let Some(window) = btn.root().and_downcast::<gtk::Window>() else {
                return;
            };
            let dialog = gtk_dialogs::Alert::new(
                Some(&window),
                Some(&crate::i18n::tr("net.confirm_removal_title")),
                Some(&crate::i18n::tr("net.confirm_removal_body")),
            );
            dialog.add_response("cancel", &crate::i18n::tr("net.cancel_btn"));
            dialog.add_response("remove", &crate::i18n::tr("net.remove_btn_confirm"));
            dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);

            let sender_resp = sender_rm.clone();
            let config_resp = config_rm.clone();
            let id_resp = id_rm.clone();
            dialog.connect_response(move |response| {
                if response == "remove" {
                    client_config::apps::remove_by_id(&config_resp, &id_resp);
                    sender_resp.input(NetworkInput::ReloadApps);
                }
            });
            dialog.present();
        });

        lst.append(&row);
    }
}

fn parse_desktop_file(path: &std::path::Path) -> Option<ProxiedApp> {
    let data = fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut no_display = false;

    for line in data.lines() {
        let line = line.trim();
        if line.starts_with("Name=") && name.is_none() {
            name = Some(line["Name=".len()..].to_string());
        } else if line.starts_with("Exec=") && exec.is_none() {
            let mut cmd = line["Exec=".len()..].to_string();
            for pat in &["%u", "%U", "%f", "%F", "%c", "%k", "%i", "%m"] {
                cmd = cmd.replace(pat, "");
            }
            exec = Some(cmd.trim().to_string());
        } else if line.starts_with("Icon=") && icon.is_none() {
            icon = Some(line["Icon=".len()..].to_string());
        } else if line.starts_with("NoDisplay=true") {
            no_display = true;
        }
    }

    if no_display {
        return None;
    }

    if let (Some(n), Some(e)) = (name, exec) {
        Some(ProxiedApp::new(n, e, icon))
    } else {
        None
    }
}

#[cfg(unix)]
fn get_all_installed_apps() -> Vec<ProxiedApp> {
    let mut apps = Vec::new();
    let mut dirs_to_scan = vec![std::path::PathBuf::from("/usr/share/applications")];

    let mut local_share = std::path::PathBuf::from(gtk::glib::home_dir().as_os_str());
    local_share.push(".local/share/applications");
    dirs_to_scan.push(local_share);

    for dir in dirs_to_scan {
        if !dir.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
                    if let Some(app) = parse_desktop_file(&path) {
                        apps.push(app);
                    }
                }
            }
        }
    }

    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.name == b.name);
    apps
}

fn persist_new_app(
    config: &client_config::AppConfig,
    sender: &ComponentSender<NetworkModel>,
    app: ProxiedApp,
) {
    client_config::apps::upsert(config, app);
    sender.input(NetworkInput::ReloadApps);
}

fn fallback_app(picked: &std::path::Path, final_path: &std::path::Path) -> ProxiedApp {
    ProxiedApp::new(
        picked
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        final_path.to_string_lossy().to_string(),
        None,
    )
}

fn proxied_app_from_path(picked: &std::path::Path, final_path: &std::path::Path) -> ProxiedApp {
    if final_path.extension().and_then(|s| s.to_str()) == Some("desktop") {
        parse_desktop_file(final_path).unwrap_or_else(|| fallback_app(picked, final_path))
    } else {
        fallback_app(picked, final_path)
    }
}

#[cfg(unix)]
fn show_app_selector_dialog(
    parent: &gtk::Window,
    config: client_config::AppConfig,
    sender: ComponentSender<NetworkModel>,
) {
    let dialog = adw::Window::builder()
        .title(&*crate::i18n::tr("net.select_application_title"))
        .modal(true)
        .transient_for(parent)
        .default_width(450)
        .default_height(600)
        .build();

    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();

    let browse_btn = gtk::Button::builder()
        .label(&*crate::i18n::tr("net.browse_btn"))
        .build();
    browse_btn.set_cursor_from_name(Some("pointer"));
    header.pack_end(&browse_btn);
    view.add_top_bar(&header);

    let search_bar = gtk::SearchEntry::builder()
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .hexpand(true)
        .build();

    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(vec!["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .build();

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&list_box)
        .build();

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_box.append(&search_bar);
    content_box.append(&scrolled);

    view.set_content(Some(&content_box));
    dialog.set_content(Some(&view));

    for app in get_all_installed_apps() {
        let row = adw::ActionRow::builder()
            .title(&app.name)
            .subtitle(&app.exec_cmd)
            .activatable(true)
            .build();

        let icon_img = gtk::Image::from_icon_name(
            app.icon_name
                .as_deref()
                .unwrap_or("application-x-executable"),
        );
        icon_img.set_pixel_size(32);
        row.add_prefix(&icon_img);

        let d_clone = dialog.clone();
        let sender_row = sender.clone();
        let config_row = config.clone();
        row.connect_activated(move |_| {
            persist_new_app(&config_row, &sender_row, app.clone());
            d_clone.destroy();
        });

        list_box.append(&row);
    }

    let search_bar_clone = search_bar.clone();
    list_box.set_filter_func(move |row| {
        if let Some(ar) = row.downcast_ref::<adw::ActionRow>() {
            let query = search_bar_clone.text().to_string().to_lowercase();
            if query.is_empty() {
                return true;
            }
            let title = ar.title().to_string().to_lowercase();
            let sub = ar.subtitle().unwrap_or_default().to_string().to_lowercase();
            return title.contains(&query) || sub.contains(&query);
        }
        true
    });
    let lb_clone = list_box.clone();
    search_bar.connect_search_changed(move |_| {
        lb_clone.invalidate_filter();
    });

    let d_clone = dialog.clone();
    let sender_browse = sender.clone();
    let config_browse = config.clone();
    browse_btn.connect_clicked(move |_| {
        let sender_resp = sender_browse.clone();
        let config_resp = config_browse.clone();
        let d2 = d_clone.clone();
        gtk_dialogs::open_file(
            Some(d_clone.upcast_ref::<gtk::Window>()),
            &crate::i18n::tr("net.select_application_title"),
            None,
            move |path| {
                #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
                let mut final_path = path.clone();

                #[cfg(target_os = "macos")]
                {
                    if path.extension().and_then(|s| s.to_str()) == Some("app") {
                        let macos_dir = path.join("Contents").join("MacOS");
                        if macos_dir.exists() && macos_dir.is_dir() {
                            if let Ok(entries) = std::fs::read_dir(&macos_dir) {
                                for entry in entries.flatten() {
                                    if let Ok(meta) = entry.metadata() {
                                        if meta.is_file() {
                                            final_path = entry.path();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                persist_new_app(
                    &config_resp,
                    &sender_resp,
                    proxied_app_from_path(&path, &final_path),
                );
                d2.destroy();
            },
        );
    });

    dialog.present();
}

#[cfg(windows)]
fn show_app_selector_dialog(
    parent: &gtk::Window,
    config: client_config::AppConfig,
    sender: ComponentSender<NetworkModel>,
) {
    gtk_dialogs::open_file(
        Some(parent),
        &crate::i18n::tr("net.select_application_title"),
        Some(gtk_dialogs::Filter::new(
            &crate::i18n::tr("net.exe_filter"),
            &["exe"],
        )),
        move |path| {
            persist_new_app(&config, &sender, proxied_app_from_path(&path, &path));
        },
    );
}
