#![allow(deprecated)]
use adw::prelude::*;
use nodeinnet_p2p::p2p::{ResourceType, SharedResource};
use relm4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub fn get_resource_icon_path() -> String {
    if cfg!(target_os = "macos") {
        "/com/rdesk-ui/gtk/desktop-mac.svg".to_string()
    } else {
        "/com/rdesk-ui/gtk/desktop-pc.svg".to_string()
    }
}

pub struct RdeskSettingsInit {
    pub existing: Option<SharedResource>,
    pub config: client_config::AppConfig,
}

#[derive(Debug)]
pub enum RdeskSettingsInput {
    InstallGst,
    InstallPortal,
    GstDone(Result<(), String>),
    PortalDone(Result<(), String>),
}

#[derive(Debug)]
pub enum RdeskSettingsOutput {
    Saved,
    Cancelled,
}

#[cfg(target_os = "linux")]
#[derive(Clone, PartialEq)]
enum DiagStatus {
    Idle,
    Installing,
    Done,
    Failed(String),
}

pub struct RdeskSettingsModel {
    #[cfg(target_os = "linux")]
    gst_available: bool,
    #[cfg(target_os = "linux")]
    portal_available: bool,
    #[cfg(target_os = "linux")]
    gst_status: DiagStatus,
    #[cfg(target_os = "linux")]
    portal_status: DiagStatus,
}

pub struct RdeskSettingsWidgets {
    #[cfg(target_os = "linux")]
    gst_lbl: gtk::Label,
    #[cfg(target_os = "linux")]
    gst_install_btn: gtk::Button,
    #[cfg(target_os = "linux")]
    portal_lbl: gtk::Label,
    #[cfg(target_os = "linux")]
    portal_install_btn: gtk::Button,
}

impl SimpleComponent for RdeskSettingsModel {
    type Init = RdeskSettingsInit;
    type Input = RdeskSettingsInput;
    type Output = RdeskSettingsOutput;
    type Root = gtk::Box;
    type Widgets = RdeskSettingsWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        crate::init_resources();
        let existing_res = init.existing;
        let config = init.config;

        let form_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&form_container)
            .vexpand(true)
            .build();
        root.append(&scrolled);

        let is_edit = existing_res.is_some();

        let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
        body.set_margin_top(24);
        body.set_margin_bottom(24);
        body.set_margin_start(24);
        body.set_margin_end(24);

        let title_text = if is_edit {
            crate::i18n::tr("common_forms.edit_service")
        } else {
            crate::i18n::tr("common_forms.add_service")
        };
        let title_lbl = gtk::Label::builder()
            .label(&*title_text)
            .halign(gtk::Align::Start)
            .css_classes(vec!["title-2", "cyber-text"])
            .build();
        body.append(&title_lbl);

        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        let name_vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
        name_vbox.set_hexpand(true);

        let name_label = gtk::Label::builder()
            .label(&*crate::i18n::tr("common_forms.resource_name"))
            .halign(gtk::Align::Start)
            .css_classes(vec!["title-4"])
            .build();
        let name_entry = gtk::Entry::builder()
            .placeholder_text(&*crate::i18n::tr("common_forms.custom_name_placeholder"))
            .text(&*crate::i18n::tr("rdesk.remote_desktop"))
            .build();

        name_vbox.append(&name_label);
        name_vbox.append(&name_entry);

        let icon_image = gtk::Image::from_resource(&get_resource_icon_path());
        icon_image.set_pixel_size(80);
        icon_image.set_margin_end(16);

        header_box.append(&icon_image);
        header_box.append(&name_vbox);
        header_box.set_margin_top(16);
        body.append(&header_box);

        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        status_box.set_margin_top(8);
        let status_lbl = gtk::Label::builder()
            .label(&*crate::i18n::tr("common_forms.service_active"))
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        let toggle_switch = gtk::Switch::builder()
            .valign(gtk::Align::Center)
            .active(true)
            .build();
        toggle_switch.set_cursor_from_name(Some("pointer"));
        status_box.append(&status_lbl);
        status_box.append(&toggle_switch);
        body.append(&status_box);

        let rdesk_form_box = gtk::Box::new(gtk::Orientation::Vertical, 16);

        let control_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let rdesk_lbl = gtk::Label::builder()
            .label(&*crate::i18n::tr("rdesk.allow_control"))
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        let rdesk_check = gtk::CheckButton::builder().active(true).build();
        rdesk_check.set_cursor_from_name(Some("pointer"));
        control_box.append(&rdesk_lbl);
        control_box.append(&rdesk_check);
        rdesk_form_box.append(&control_box);

        #[cfg(target_os = "linux")]
        let (gst_lbl, gst_install_btn, portal_lbl, portal_install_btn) = {
            rdesk_form_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            let diag_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
            diag_box.set_margin_top(8);

            let diag_title = gtk::Label::builder()
                .label(&*crate::i18n::tr("rdesk.host_diagnostics"))
                .halign(gtk::Align::Start)
                .css_classes(vec!["title-4"])
                .build();
            diag_box.append(&diag_title);

            let diag_desc = gtk::Label::builder()
                .label(&*crate::i18n::tr("rdesk.diag_desc"))
                .halign(gtk::Align::Start)
                .css_classes(vec!["dim-label"])
                .wrap(true)
                .build();
            diag_box.append(&diag_desc);

            let gst_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            gst_row.set_margin_top(8);
            let gst_lbl = gtk::Label::builder().halign(gtk::Align::Start).build();
            let gst_install_btn = gtk::Button::builder()
                .label(&*crate::i18n::tr("rdesk.install_gst"))
                .css_classes(vec!["suggested-action", "pill"])
                .valign(gtk::Align::Center)
                .build();
            gst_install_btn.set_cursor_from_name(Some("pointer"));
            gst_row.append(&gst_lbl);
            gst_row.append(&gst_install_btn);
            diag_box.append(&gst_row);

            let portal_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            portal_row.set_margin_top(8);
            let portal_lbl = gtk::Label::builder().halign(gtk::Align::Start).build();
            let portal_install_btn = gtk::Button::builder()
                .label(&*crate::i18n::tr("rdesk.install_portal"))
                .css_classes(vec!["suggested-action", "pill"])
                .valign(gtk::Align::Center)
                .build();
            portal_install_btn.set_cursor_from_name(Some("pointer"));
            portal_row.append(&portal_lbl);
            portal_row.append(&portal_install_btn);
            diag_box.append(&portal_row);

            let sender_gst = sender.clone();
            gst_install_btn.connect_clicked(move |_| {
                sender_gst.input(RdeskSettingsInput::InstallGst);
            });
            let sender_portal = sender.clone();
            portal_install_btn.connect_clicked(move |_| {
                sender_portal.input(RdeskSettingsInput::InstallPortal);
            });

            rdesk_form_box.append(&diag_box);
            (gst_lbl, gst_install_btn, portal_lbl, portal_install_btn)
        };

        #[cfg(target_os = "macos")]
        {
            rdesk_form_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            let diag_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
            diag_box.set_margin_top(8);

            let diag_title = gtk::Label::builder()
                .label(&*crate::i18n::tr("rdesk.host_diagnostics"))
                .halign(gtk::Align::Start)
                .css_classes(vec!["title-4"])
                .build();
            diag_box.append(&diag_title);

            let diag_desc = gtk::Label::builder()
                .label(&*crate::i18n::tr("rdesk.diag_desc_mac"))
                .halign(gtk::Align::Start)
                .css_classes(vec!["dim-label"])
                .wrap(true)
                .build();
            diag_box.append(&diag_desc);

            let screen_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            screen_row.set_margin_top(8);
            let screen_lbl = gtk::Label::builder()
                .label(&*crate::i18n::tr("rdesk.mac_screen_capture"))
                .halign(gtk::Align::Start)
                .build();
            let screen_btn = gtk::Button::builder()
                .label(&*crate::i18n::tr("rdesk.configure_btn"))
                .css_classes(vec!["suggested-action", "pill"])
                .valign(gtk::Align::Center)
                .build();
            screen_btn.set_cursor_from_name(Some("pointer"));
            screen_row.append(&screen_lbl);
            screen_row.append(&screen_btn);
            diag_box.append(&screen_row);

            screen_btn.connect_clicked(|_| {
                let _ = std::process::Command::new("open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                    .spawn();
            });

            let access_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            access_row.set_margin_top(8);
            let access_lbl = gtk::Label::builder()
                .label(&*crate::i18n::tr("rdesk.mac_accessibility"))
                .halign(gtk::Align::Start)
                .build();
            let access_btn = gtk::Button::builder()
                .label(&*crate::i18n::tr("rdesk.configure_btn"))
                .css_classes(vec!["suggested-action", "pill"])
                .valign(gtk::Align::Center)
                .build();
            access_btn.set_cursor_from_name(Some("pointer"));
            access_row.append(&access_lbl);
            access_row.append(&access_btn);
            diag_box.append(&access_row);

            access_btn.connect_clicked(|_| {
                let _ = std::process::Command::new("open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                    .spawn();
            });

            rdesk_form_box.append(&diag_box);
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            rdesk_form_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            let diag_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
            diag_box.set_margin_top(8);

            let diag_title = gtk::Label::builder()
                .label(&*crate::i18n::tr("rdesk.host_diagnostics"))
                .halign(gtk::Align::Start)
                .css_classes(vec!["title-4"])
                .build();
            diag_box.append(&diag_title);

            let diag_desc = gtk::Label::builder()
                .label(&*crate::i18n::tr("rdesk.diag_desc_ready"))
                .halign(gtk::Align::Start)
                .css_classes(vec!["dim-label"])
                .wrap(true)
                .build();
            diag_box.append(&diag_desc);

            rdesk_form_box.append(&diag_box);
        }

        body.append(&rdesk_form_box);

        if let Some(ref res) = existing_res {
            toggle_switch.set_active(res.is_active);

            if let Some(cfg_str) = &res.config {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(cfg_str) {
                    if res.resource_type == ResourceType::RemoteDesktop {
                        let allow_control = v
                            .get("allow_control")
                            .and_then(|ac| ac.as_bool())
                            .unwrap_or(true);
                        rdesk_check.set_active(allow_control);
                    }
                }
            }

            name_entry.set_text(&res.name);
        }

        let is_deleted = Rc::new(RefCell::new(false));

        let do_save = {
            let name_entry = name_entry.clone();
            let rdesk_check = rdesk_check.clone();
            let toggle_switch = toggle_switch.clone();
            let existing_res = existing_res.clone();
            let is_deleted_cl = is_deleted.clone();
            let config = config.clone();

            Rc::new(move || -> bool {
                if *is_deleted_cl.borrow() {
                    return true;
                }

                let mut r_name = name_entry.text().to_string();
                if r_name.trim().is_empty() {
                    r_name = crate::i18n::tr("rdesk.remote_desktop");
                }

                let ac = rdesk_check.is_active();
                let config_json = Some(format!(r#"{{"allow_control": {}}}"#, ac));

                let mut r = config
                    .get::<Vec<SharedResource>>("app.resources")
                    .unwrap_or_default();

                if let Some(ref ext_res) = existing_res {
                    if let Some(target) = r.iter_mut().find(|x| x.id == ext_res.id) {
                        target.name = r_name;
                        target.config = config_json;
                        target.is_active = toggle_switch.is_active();
                    }
                } else {
                    r.retain(|x| x.resource_type != ResourceType::RemoteDesktop);
                    r.push(SharedResource {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: r_name,
                        resource_type: ResourceType::RemoteDesktop,
                        config: config_json,
                        is_active: toggle_switch.is_active(),
                        session_token: None,
                    });
                }

                config.set("app.resources", r);
                config.save();
                true
            })
        };

        let buttons_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        buttons_box.set_margin_top(32);
        buttons_box.set_halign(gtk::Align::End);

        let cancel_btn = gtk::Button::builder()
            .label(&*crate::i18n::tr("common_forms.cancel"))
            .css_classes(vec!["pill-button"])
            .build();
        cancel_btn.set_cursor_from_name(Some("pointer"));
        let save_btn = gtk::Button::builder()
            .label(&*crate::i18n::tr("common_forms.save"))
            .css_classes(vec!["suggested-action", "pill-button"])
            .build();
        save_btn.set_cursor_from_name(Some("pointer"));

        let sender_cancel = sender.clone();
        cancel_btn.connect_clicked(move |_| {
            let _ = sender_cancel.output(RdeskSettingsOutput::Cancelled);
        });

        let sender_save = sender.clone();
        let do_save_cl = do_save.clone();
        save_btn.connect_clicked(move |_| {
            if do_save_cl() {
                let _ = sender_save.output(RdeskSettingsOutput::Saved);
            }
        });

        if is_edit {
            let del_btn = gtk::Button::builder()
                .label(&*crate::i18n::tr("common_forms.delete_service_btn"))
                .css_classes(vec!["destructive-action", "pill-button"])
                .build();
            del_btn.set_cursor_from_name(Some("pointer"));

            let ext_res = existing_res.clone().unwrap();
            let res_id = ext_res.id.clone();
            let res_name = ext_res.name.clone();
            let is_deleted_inner = is_deleted.clone();
            let sender_del = sender.clone();
            let root_cl = root.clone();
            let config_del = config.clone();
            del_btn.connect_clicked(move |_| {
                let win_del = root_cl.root().and_downcast::<gtk::Window>();
                let mut builder = adw::MessageDialog::builder()
                    .heading(&*crate::i18n::tr("common_forms.delete_service_title"))
                    .body(&*crate::i18n::trf(
                        "common_forms.delete_service_text",
                        &[("name", &*(res_name).to_string())],
                    ));
                if let Some(ref w) = win_del {
                    builder = builder.transient_for(w);
                }
                let confirm_dialog = builder.build();

                let icon = gtk::Image::from_resource(&get_resource_icon_path());
                icon.set_pixel_size(40);
                icon.set_margin_bottom(16);
                confirm_dialog.set_extra_child(Some(&icon));

                confirm_dialog.add_response("cancel", &crate::i18n::tr("common_forms.cancel"));
                confirm_dialog.add_response("delete", &crate::i18n::tr("common_forms.delete"));
                confirm_dialog
                    .set_response_appearance("delete", adw::ResponseAppearance::Destructive);

                let sender_inner = sender_del.clone();
                let is_del_flag = is_deleted_inner.clone();
                let res_id_cl = res_id.clone();
                let config_del_inner = config_del.clone();
                confirm_dialog.connect_response(None, move |dlg, response| {
                    if response != "delete" {
                        return;
                    }
                    *is_del_flag.borrow_mut() = true;

                    let mut r = config_del_inner
                        .get::<Vec<SharedResource>>("app.resources")
                        .unwrap_or_default();
                    r.retain(|x| x.id != res_id_cl);
                    config_del_inner.set("app.resources", r);
                    config_del_inner.save();

                    dlg.close();
                    let _ = sender_inner.output(RdeskSettingsOutput::Saved);
                });
                confirm_dialog.present();
            });

            let edit_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            edit_buttons.set_margin_top(32);
            edit_buttons.append(&del_btn);
            let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            spacer.set_hexpand(true);
            edit_buttons.append(&spacer);
            edit_buttons.append(&cancel_btn);
            edit_buttons.append(&save_btn);
            body.append(&edit_buttons);
        } else {
            buttons_box.append(&cancel_btn);
            buttons_box.append(&save_btn);
            body.append(&buttons_box);
        }

        form_container.append(&body);

        let model = RdeskSettingsModel {
            #[cfg(target_os = "linux")]
            gst_available: node_functions::desktop::is_gstreamer_pipewire_available(),
            #[cfg(target_os = "linux")]
            portal_available: node_functions::desktop::is_xdg_desktop_portal_installed(),
            #[cfg(target_os = "linux")]
            gst_status: DiagStatus::Idle,
            #[cfg(target_os = "linux")]
            portal_status: DiagStatus::Idle,
        };
        let widgets = RdeskSettingsWidgets {
            #[cfg(target_os = "linux")]
            gst_lbl,
            #[cfg(target_os = "linux")]
            gst_install_btn,
            #[cfg(target_os = "linux")]
            portal_lbl,
            #[cfg(target_os = "linux")]
            portal_install_btn,
        };
        ComponentParts { model, widgets }
    }

    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        #[cfg(target_os = "linux")]
        match message {
            RdeskSettingsInput::InstallGst => {
                if self.gst_status == DiagStatus::Installing {
                    return;
                }
                self.gst_status = DiagStatus::Installing;
                let sender = sender.clone();
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Runtime::new() {
                        rt.block_on(async {
                            let res = node_functions::desktop::run_gstreamer_installer().await;
                            if res.is_ok() {
                                node_functions::desktop::update_gstreamer_registry();
                            }
                            sender
                                .input(RdeskSettingsInput::GstDone(res.map_err(|e| e.to_string())));
                        });
                    }
                });
            }
            RdeskSettingsInput::InstallPortal => {
                if self.portal_status == DiagStatus::Installing {
                    return;
                }
                self.portal_status = DiagStatus::Installing;
                let sender = sender.clone();
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Runtime::new() {
                        rt.block_on(async {
                            let res = node_functions::desktop::run_portal_installer().await;
                            sender.input(RdeskSettingsInput::PortalDone(
                                res.map_err(|e| e.to_string()),
                            ));
                        });
                    }
                });
            }
            RdeskSettingsInput::GstDone(res) => {
                self.gst_status = match res {
                    Ok(()) => DiagStatus::Done,
                    Err(e) => DiagStatus::Failed(e),
                };
                self.gst_available = node_functions::desktop::is_gstreamer_pipewire_available();
                self.portal_available = node_functions::desktop::is_xdg_desktop_portal_installed();
            }
            RdeskSettingsInput::PortalDone(res) => {
                self.portal_status = match res {
                    Ok(()) => DiagStatus::Done,
                    Err(e) => DiagStatus::Failed(e),
                };
                self.gst_available = node_functions::desktop::is_gstreamer_pipewire_available();
                self.portal_available = node_functions::desktop::is_xdg_desktop_portal_installed();
            }
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        #[cfg(target_os = "linux")]
        {
            match &self.gst_status {
                DiagStatus::Installing => {
                    widgets
                        .gst_lbl
                        .set_markup(&crate::i18n::tr("rdesk.installing_gst"));
                }
                DiagStatus::Done => {
                    widgets
                        .gst_lbl
                        .set_markup(&crate::i18n::tr("rdesk.gst_success"));
                }
                DiagStatus::Failed(e) => {
                    widgets.gst_lbl.set_markup(&crate::i18n::trf(
                        "rdesk.install_failed",
                        &[("error", &*(e).to_string())],
                    ));
                }
                DiagStatus::Idle => {
                    widgets
                        .gst_lbl
                        .set_markup(&crate::i18n::tr(if self.gst_available {
                            "rdesk.gst_available"
                        } else {
                            "rdesk.gst_missing"
                        }));
                }
            }
            widgets.gst_install_btn.set_visible(!self.gst_available);
            widgets
                .gst_install_btn
                .set_sensitive(self.gst_status != DiagStatus::Installing);

            match &self.portal_status {
                DiagStatus::Installing => {
                    widgets
                        .portal_lbl
                        .set_markup(&crate::i18n::tr("rdesk.installing_portal"));
                }
                DiagStatus::Done => {
                    widgets
                        .portal_lbl
                        .set_markup(&crate::i18n::tr("rdesk.portal_success"));
                }
                DiagStatus::Failed(e) => {
                    widgets.portal_lbl.set_markup(&crate::i18n::trf(
                        "rdesk.install_failed",
                        &[("error", &*(e).to_string())],
                    ));
                }
                DiagStatus::Idle => {
                    widgets
                        .portal_lbl
                        .set_markup(&crate::i18n::tr(if self.portal_available {
                            "rdesk.portal_available"
                        } else {
                            "rdesk.portal_missing"
                        }));
                }
            }
            widgets
                .portal_install_btn
                .set_visible(!self.portal_available);
            widgets
                .portal_install_btn
                .set_sensitive(self.portal_status != DiagStatus::Installing);
        }
    }
}
