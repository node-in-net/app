#![allow(deprecated)]
use adw::prelude::*;
use relm4::ComponentSender;

use crate::fm_view::{ClipAction, FmPanelModel, FmPanelOutput, Shared};

fn unique_copy_name(name: &str, is_dir: bool, existing: &[String]) -> String {
    let (stem, ext) = match (is_dir, name.rfind('.')) {
        (false, Some(i)) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            format!("{stem} copy{ext}")
        } else {
            format!("{stem} copy {n}{ext}")
        };
        if !existing.iter().any(|e| e == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_context_menu(
    name: &str,
    _size: u64,
    is_dir: bool,
    parent_path: String,
    shared: &Shared,
    root_path: String,
    sender: &ComponentSender<FmPanelModel>,
    btn_delete: &gtk::Button,
    _btn_refresh: &gtk::Button,
    btn_chmod: &gtk::Button,
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .position(gtk::PositionType::Bottom)
        .has_arrow(true)
        .build();
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
    vbox.set_margin_start(4);
    vbox.set_margin_end(4);
    vbox.set_margin_top(4);
    vbox.set_margin_bottom(4);
    let name_clone = name.to_string();
    let safe_parent = if parent_path.ends_with('/') {
        parent_path.clone()
    } else {
        format!("{}/", parent_path)
    };

    let create_btn_with_label = |label: &str, icon: &str, css: &str| -> gtk::Button {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        b.append(&gtk::Image::from_icon_name(icon));
        b.append(&gtk::Label::new(Some(label)));
        let mut classes = vec!["flat"];
        if !css.is_empty() {
            classes.push(css);
        }
        gtk::Button::builder()
            .child(&b)
            .css_classes(classes)
            .build()
    };

    let btn_copy = create_btn_with_label(
        &crate::i18n::tr("fm.context.copy_path"),
        "edit-copy-symbolic",
        "",
    );
    let pop_copy = popover.clone();
    let name_c = name_clone.clone();
    let root_path_c = root_path.clone();
    let safe_parent_c = safe_parent.clone();
    btn_copy.connect_clicked(move |_| {
        pop_copy.popdown();
        let full_item_path = format!("{}{}", safe_parent_c, name_c);
        let copy_text = relative_to_root(&full_item_path, &root_path_c);

        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&copy_text);
        }
    });
    vbox.append(&btn_copy);
    vbox.append(
        &gtk::Separator::builder()
            .orientation(gtk::Orientation::Horizontal)
            .build(),
    );

    for (label, icon, action, into) in [
        (
            crate::i18n::tr("fm.context.cut"),
            "edit-cut-symbolic",
            ClipAction::Cut,
            None,
        ),
        (
            crate::i18n::tr("fm.context.copy"),
            "edit-copy-symbolic",
            ClipAction::Copy,
            None,
        ),
        (
            crate::i18n::tr("fm.context.paste_here"),
            "edit-paste-symbolic",
            ClipAction::Paste,
            None,
        ),
        (
            crate::i18n::trf("fm.context.paste_into", &[("name", name)]),
            "edit-paste-symbolic",
            ClipAction::Paste,
            Some(name.to_string()),
        ),
    ] {
        if into.is_some() && !is_dir {
            continue;
        }
        let btn = create_btn_with_label(&label, icon, "");
        if action == ClipAction::Paste {
            btn.set_sensitive(shared.clipboard_held.get() > 0);
        }
        let pop = popover.clone();
        let sender = sender.clone();
        btn.connect_clicked(move |_| {
            pop.popdown();
            let _ = sender.output(FmPanelOutput::Clip {
                action,
                into: into.clone(),
            });
        });
        vbox.append(&btn);
    }
    vbox.append(
        &gtk::Separator::builder()
            .orientation(gtk::Orientation::Horizontal)
            .build(),
    );

    if !is_dir {
        let btn_dl = create_btn_with_label(
            &crate::i18n::tr("fm.context.download"),
            "folder-download-symbolic",
            "",
        );
        let f_path = format!("{}{}", safe_parent, name_clone);
        let pop_dl = popover.clone();
        let sender_dl = sender.clone();
        btn_dl.connect_clicked(move |_| {
            let _ = sender_dl.output(FmPanelOutput::Download {
                paths: vec![f_path.clone()],
            });
            pop_dl.popdown();
        });
        vbox.append(&btn_dl);
    }

    let btn_ren = create_btn_with_label(
        &crate::i18n::tr("fm.context.rename"),
        "document-edit-symbolic",
        "",
    );
    let pop_ren = popover.clone();
    let sender_ren = sender.clone();
    let parent_ren = parent_path.clone();
    let name_ren = name_clone.clone();
    btn_ren.connect_clicked(move |_| {
        pop_ren.popdown();
        if let Some(window) = pop_ren
            .parent()
            .and_then(|p| p.root())
            .and_downcast::<gtk::Window>()
        {
            crate::dialogs::rename_dialog_for(&window, &parent_ren, &name_ren, sender_ren.clone());
        }
    });
    vbox.append(&btn_ren);

    let btn_dup = create_btn_with_label(
        &crate::i18n::tr("fm.context.duplicate"),
        "edit-copy-symbolic",
        "",
    );
    let pop_dup = popover.clone();
    let sender_dup = sender.clone();
    let dup_parent = safe_parent.clone();
    let dup_name = name_clone.clone();
    let existing_names: Vec<String> = shared
        .cached_entries
        .borrow()
        .iter()
        .map(|(n, _, _, _, _)| n.clone())
        .collect();
    btn_dup.connect_clicked(move |_| {
        pop_dup.popdown();
        let dest_name = unique_copy_name(&dup_name, is_dir, &existing_names);
        let _ = sender_dup.output(FmPanelOutput::Duplicate {
            src: format!("{}{}", dup_parent, dup_name),
            dst: format!("{}{}", dup_parent, dest_name),
        });
    });
    vbox.append(&btn_dup);

    let btn_del = create_btn_with_label(
        &crate::i18n::tr("fm.context.delete"),
        "user-trash-symbolic",
        "destructive-action",
    );
    let pop_del = popover.clone();
    let btn_delete_clone = btn_delete.clone();
    btn_del.connect_clicked(move |_| {
        pop_del.popdown();
        btn_delete_clone.activate();
    });
    vbox.append(&btn_del);

    let btn_chm = create_btn_with_label(
        &crate::i18n::tr("fm.context.permissions"),
        "dialog-password-symbolic",
        "",
    );
    let pop_chm = popover.clone();
    let btn_chmod_clone = btn_chmod.clone();
    btn_chm.connect_clicked(move |_| {
        pop_chm.popdown();
        btn_chmod_clone.activate();
    });
    vbox.append(&btn_chm);

    popover.set_child(Some(&vbox));
    popover
}

pub fn create_empty_space_menu(
    shared: &Shared,
    sender: &ComponentSender<FmPanelModel>,
) -> gtk::Popover {
    let popover = gtk::Popover::builder()
        .position(gtk::PositionType::Bottom)
        .has_arrow(true)
        .build();
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
    vbox.set_margin_start(4);
    vbox.set_margin_end(4);
    vbox.set_margin_top(4);
    vbox.set_margin_bottom(4);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&gtk::Image::from_icon_name("edit-paste-symbolic"));
    row.append(&gtk::Label::new(Some(&crate::i18n::tr(
        "fm.context.paste_here",
    ))));
    let btn = gtk::Button::builder()
        .child(&row)
        .css_classes(vec!["flat"])
        .build();
    btn.set_sensitive(shared.clipboard_held.get() > 0);
    let pop = popover.clone();
    let sender = sender.clone();
    btn.connect_clicked(move |_| {
        pop.popdown();
        let _ = sender.output(FmPanelOutput::Clip {
            action: ClipAction::Paste,
            into: None,
        });
    });
    vbox.append(&btn);
    popover.set_child(Some(&vbox));
    popover
}

fn relative_to_root(full_item_path: &str, root_path: &str) -> String {
    if !root_path.is_empty() && root_path != "/" && full_item_path.starts_with(root_path) {
        let mut rel = full_item_path[root_path.len()..].to_string();
        if !rel.starts_with('/') {
            rel = format!("/{}", rel);
        }
        rel
    } else {
        full_item_path.to_string()
    }
}
