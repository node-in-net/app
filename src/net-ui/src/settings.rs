#![allow(deprecated)]
use adw::prelude::*;
use nodeinnet_p2p::p2p::{ResourceType, SharedResource};
use relm4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub fn get_resource_icon_path() -> String {
    "/com/net-ui/gtk/vpn.svg".to_string()
}

const ALLOW_LOCAL_KEY: &str = "allow_local_network";

fn read_allow_local(res: &SharedResource) -> bool {
    res.config
        .as_ref()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get(ALLOW_LOCAL_KEY).and_then(|b| b.as_bool()))
        .unwrap_or(false)
}

fn write_allow_local(allow: bool) -> Option<String> {
    serde_json::to_string(&serde_json::json!({ ALLOW_LOCAL_KEY: allow })).ok()
}

pub struct NetSettingsInit {
    pub existing: Option<SharedResource>,
    pub config: client_config::AppConfig,
}

#[derive(Debug)]
pub enum NetSettingsInput {}

#[derive(Debug)]
pub enum NetSettingsOutput {
    Saved,
    Cancelled,
}

pub struct NetSettingsModel {}

impl SimpleComponent for NetSettingsModel {
    type Init = NetSettingsInit;
    type Input = NetSettingsInput;
    type Output = NetSettingsOutput;
    type Root = gtk::Box;
    type Widgets = ();

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

        let label_text = if is_edit {
            crate::i18n::tr("common_forms.edit_service")
        } else {
            crate::i18n::tr("common_forms.add_service")
        };
        let title_lbl = gtk::Label::builder()
            .label(&*label_text)
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
            .text(&*crate::i18n::tr("net.gateway_name"))
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

        let lan_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        lan_box.set_margin_top(8);
        let lan_lbl = gtk::Label::builder()
            .label(&*crate::i18n::tr("net_settings.allow_local_network"))
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        let lan_switch = gtk::Switch::builder()
            .valign(gtk::Align::Center)
            .active(false)
            .build();
        lan_switch.set_cursor_from_name(Some("pointer"));
        lan_box.append(&lan_lbl);
        lan_box.append(&lan_switch);
        body.append(&lan_box);

        let gen_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        gen_box.append(
            &gtk::Label::builder()
                .label(&*crate::i18n::tr("net_settings.allow_local_network_hint"))
                .css_classes(vec!["dim-label"])
                .wrap(true)
                .xalign(0.0)
                .build(),
        );
        body.append(&gen_box);

        if let Some(ref res) = existing_res {
            toggle_switch.set_active(res.is_active);
            name_entry.set_text(&res.name);
            lan_switch.set_active(read_allow_local(res));
        }

        let is_deleted = Rc::new(RefCell::new(false));

        let do_save = {
            let name_entry = name_entry.clone();
            let toggle_switch = toggle_switch.clone();
            let lan_switch = lan_switch.clone();
            let existing_res = existing_res.clone();
            let is_deleted_cl = is_deleted.clone();
            let config = config.clone();

            Rc::new(move || -> bool {
                if *is_deleted_cl.borrow() {
                    return true;
                }

                let mut r_name = name_entry.text().to_string();
                if r_name.trim().is_empty() {
                    r_name = crate::i18n::tr("net.gateway_name");
                }

                let mut r = config
                    .get::<Vec<SharedResource>>("app.resources")
                    .unwrap_or_default();

                if let Some(ref ext_res) = existing_res {
                    if let Some(target) = r.iter_mut().find(|x| x.id == ext_res.id) {
                        target.name = r_name;
                        target.config = write_allow_local(lan_switch.is_active());
                        target.is_active = toggle_switch.is_active();
                    }
                } else {
                    r.retain(|x| x.resource_type != ResourceType::SharedNetwork);
                    r.push(SharedResource {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: r_name,
                        resource_type: ResourceType::SharedNetwork,
                        config: write_allow_local(lan_switch.is_active()),
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
            let _ = sender_cancel.output(NetSettingsOutput::Cancelled);
        });

        let sender_save = sender.clone();
        let do_save_cl = do_save.clone();
        save_btn.connect_clicked(move |_| {
            if do_save_cl() {
                let _ = sender_save.output(NetSettingsOutput::Saved);
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
                    let _ = sender_inner.output(NetSettingsOutput::Saved);
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

        ComponentParts {
            model: NetSettingsModel {},
            widgets: (),
        }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {}
    }
}
