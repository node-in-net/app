#![allow(deprecated)]
use adw::prelude::*;
use nodeinnet_p2p::p2p::{ResourceType, SharedResource};
use relm4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub fn get_resource_icon_path() -> String {
    "/com/sync-ui/gtk/sync-folder.svg".to_string()
}

pub struct SyncSettingsInit {
    pub existing: Option<SharedResource>,
    pub config: client_config::AppConfig,
}

#[derive(Debug)]
pub enum SyncSettingsInput {}

#[derive(Debug)]
pub enum SyncSettingsOutput {
    Saved,
    Cancelled,
}

pub struct SyncSettingsModel {}

impl SimpleComponent for SyncSettingsModel {
    type Init = SyncSettingsInit;
    type Input = SyncSettingsInput;
    type Output = SyncSettingsOutput;
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

        let title_lbl_text = if is_edit {
            crate::i18n::tr("common_forms.edit_service")
        } else {
            crate::i18n::tr("common_forms.add_service")
        };
        let title_lbl = gtk::Label::builder()
            .label(&*title_lbl_text)
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
            .text(&*crate::i18n::tr("sync.folder_name"))
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

        let sync_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let sync_fold_btn = gtk::Button::with_label(&crate::i18n::tr("common_forms.select_folder"));
        sync_fold_btn.set_cursor_from_name(Some("pointer"));
        let sync_fs_entry = gtk::Entry::builder()
            .placeholder_text(&*crate::i18n::tr("common_forms.absolute_path_placeholder"))
            .build();
        let sync_ignores_entry = gtk::Entry::builder()
            .placeholder_text(&*crate::i18n::tr("common_forms.ignores_placeholder"))
            .tooltip_text(&*crate::i18n::tr("common_forms.ignores_tooltip"))
            .build();
        let sync_error_label = gtk::Label::builder()
            .visible(false)
            .css_classes(vec!["error"])
            .wrap(true)
            .xalign(0.0)
            .build();

        let sync_policy_model = gtk::StringList::new(&[
            &crate::i18n::tr("common_forms.strategy_none"),
            &crate::i18n::tr("common_forms.strategy_source"),
            &crate::i18n::tr("common_forms.strategy_remote"),
            &crate::i18n::tr("common_forms.strategy_twoway"),
        ]);
        let sync_policy_combo = gtk::DropDown::builder()
            .model(&sync_policy_model)
            .valign(gtk::Align::Center)
            .build();
        let source_warning_label = gtk::Label::builder()
            .label(&*crate::i18n::tr("common_forms.source_warning"))
            .css_classes(vec!["warning"])
            .wrap(true)
            .visible(false)
            .xalign(0.0)
            .build();
        let source_confirm_check = gtk::CheckButton::builder()
            .label(&*crate::i18n::tr("common_forms.understand"))
            .visible(false)
            .build();

        let warn_lbl_cl = source_warning_label.clone();
        let chk_cl = source_confirm_check.clone();
        sync_policy_combo.connect_selected_notify(move |cb| {
            let is_source = cb.selected() == 1;
            warn_lbl_cl.set_visible(is_source);
            chk_cl.set_visible(is_source);
        });

        sync_box.append(
            &gtk::Label::builder()
                .label(&*crate::i18n::tr("common_forms.default_strategy"))
                .halign(gtk::Align::Start)
                .build(),
        );
        sync_box.append(&sync_fold_btn);
        sync_box.append(&sync_fs_entry);
        sync_box.append(&sync_ignores_entry);
        sync_box.append(&sync_policy_combo);
        sync_box.append(&source_warning_label);
        sync_box.append(&source_confirm_check);
        sync_box.append(&sync_error_label);

        let mut original_sync_path = String::new();
        if let Some(res) = &existing_res {
            if res.resource_type == ResourceType::SyncFolder {
                if let Some(cfg_str) = &res.config {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(cfg_str) {
                        if let Some(path) = json.get("path").and_then(|v| v.as_str()) {
                            original_sync_path = path.to_string();
                        }
                    }
                }
            }
        }

        let err_lbl_cl = sync_error_label.clone();
        let orig_sync_path_cl = original_sync_path.clone();
        sync_fs_entry.connect_changed(move |entry| {
            let path_str = entry.text().to_string();
            if path_str.is_empty() || path_str == orig_sync_path_cl {
                err_lbl_cl.set_visible(false);
                return;
            }
            let path = std::path::Path::new(&path_str);
            if path.exists() && path.is_dir() {
                let is_empty = std::fs::read_dir(path)
                    .map(|mut iter| iter.next().is_none())
                    .unwrap_or(true);
                if !is_empty {
                    err_lbl_cl.set_text(&crate::i18n::tr("common_forms.folder_nonempty_error"));
                    err_lbl_cl.set_visible(true);
                } else {
                    err_lbl_cl.set_visible(false);
                }
            } else {
                err_lbl_cl.set_visible(false);
            }
        });

        let sync_val_cl_dialog = sync_fs_entry.clone();
        sync_fold_btn.connect_clicked(move |btn| {
            let parent_win = btn.root().and_downcast::<gtk::Window>();
            let fsc = sync_val_cl_dialog.clone();
            gtk_dialogs::select_folder(
                parent_win.as_ref(),
                &crate::i18n::tr("common_forms.select_sync_folder"),
                move |path| {
                    fsc.set_text(&path.to_string_lossy());
                    fsc.emit_by_name::<()>("changed", &[]);
                },
            );
        });
        body.append(&sync_box);

        if let Some(ref res) = existing_res {
            toggle_switch.set_active(res.is_active);

            if let Some(cfg_str) = &res.config {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(cfg_str) {
                    if res.resource_type == ResourceType::SyncFolder {
                        if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
                            sync_fs_entry.set_text(p);
                        }
                        if let Some(ig_arr) = v.get("ignores").and_then(|i| i.as_array()) {
                            let ig_str = ig_arr
                                .iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            sync_ignores_entry.set_text(&ig_str);
                        }
                        if let Some(p) = v.get("default_policy").and_then(|p| p.as_u64()) {
                            sync_policy_combo.set_selected(p as u32);
                        }
                    }
                }
            }

            name_entry.set_text(&res.name);
        }

        let is_deleted = Rc::new(RefCell::new(false));

        let do_save = {
            let name_entry = name_entry.clone();
            let sync_fs_entry = sync_fs_entry.clone();
            let sync_ignores_entry = sync_ignores_entry.clone();
            let sync_policy_combo = sync_policy_combo.clone();
            let source_confirm_check = source_confirm_check.clone();
            let toggle_switch = toggle_switch.clone();
            let sync_error_label = sync_error_label.clone();
            let existing_res = existing_res.clone();
            let is_deleted_cl = is_deleted.clone();
            let config = config.clone();

            Rc::new(move || -> bool {
                if *is_deleted_cl.borrow() {
                    return true;
                }

                let mut r_name = name_entry.text().to_string();
                if r_name.trim().is_empty() {
                    r_name = crate::i18n::tr("sync.folder_name");
                }

                if sync_error_label.is_visible() {
                    return false;
                }
                let path = sync_fs_entry.text().to_string();
                let ignores_raw = sync_ignores_entry.text().to_string();
                if path.is_empty() {
                    return false;
                }
                let ignores_vec: Vec<String> = ignores_raw
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let ignores_json =
                    serde_json::to_string(&ignores_vec).unwrap_or_else(|_| "[]".to_string());
                let default_policy = sync_policy_combo.selected();
                if default_policy == 1 && !source_confirm_check.is_active() {
                    sync_error_label.set_text(&crate::i18n::tr("common_forms.understand_error"));
                    sync_error_label.set_visible(true);
                    return false;
                }
                let config_json = Some(format!(
                    r#"{{"path": "{}", "ignores": {}, "default_policy": {}}}"#,
                    path.replace('\\', "\\\\"),
                    ignores_json,
                    default_policy
                ));

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
                    r.push(SharedResource {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: r_name,
                        resource_type: ResourceType::SyncFolder,
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
            let _ = sender_cancel.output(SyncSettingsOutput::Cancelled);
        });

        let sender_save = sender.clone();
        let do_save_cl = do_save.clone();
        save_btn.connect_clicked(move |_| {
            if do_save_cl() {
                let _ = sender_save.output(SyncSettingsOutput::Saved);
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
                    let _ = sender_inner.output(SyncSettingsOutput::Saved);
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
            model: SyncSettingsModel {},
            widgets: (),
        }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {}
    }
}
