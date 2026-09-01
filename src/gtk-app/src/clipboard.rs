use adw::prelude::*;
use app_core::fm::{ClipboardView, OnConflict, Side, TransferKind, TransferPlan};
use app_headless::{ApiCmd, TransferOrigin};
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use crate::i18n::{tr, trf};
use crate::icons;

async fn ask(dialog: &adw::AlertDialog, parent: &gtk::Window) -> String {
    let (tx, rx) = oneshot::channel();
    let tx = RefCell::new(Some(tx));
    dialog.connect_response(None, move |_, resp| {
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(resp.to_string());
        }
    });
    dialog.present(Some(parent));
    rx.await.unwrap_or_else(|_| "cancel".into())
}

fn name_list(names: &[String]) -> gtk::Widget {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 2);
    for n in names.iter().take(12) {
        let l = gtk::Label::new(Some(n));
        l.set_halign(gtk::Align::Start);
        l.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        l.set_max_width_chars(48);
        col.append(&l);
    }
    if names.len() > 12 {
        let more = gtk::Label::new(Some(&trf(
            "files.and_more",
            &[("count", &(names.len() - 12).to_string())],
        )));
        more.add_css_class("dim-label");
        more.set_halign(gtk::Align::Start);
        col.append(&more);
    }
    col.upcast()
}

async fn confirm_plan(plan: &TransferPlan, window: &gtk::Window) -> bool {
    let title = tr(match plan.kind {
        TransferKind::Copy => "files.copy_title",
        TransferKind::Move => "files.move_title",
    });
    let dialog = adw::AlertDialog::new(
        Some(&trf(
            match plan.kind {
                TransferKind::Copy => "files.copy_n",
                TransferKind::Move => "files.move_n",
            },
            &[("count", &plan.items.len().to_string())],
        )),
        Some(&trf(
            "files.transfer_route",
            &[("from", &plan.from), ("to", &plan.to)],
        )),
    );
    let names: Vec<String> = plan.items.iter().map(|i| i.name.clone()).collect();
    dialog.set_extra_child(Some(&name_list(&names)));
    dialog.add_response("cancel", &tr("files.cancel"));
    dialog.add_response("ok", &title);
    dialog.set_default_response(Some("ok"));
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    ask(&dialog, window).await == "ok"
}

async fn resolve_conflicts(
    plan: &TransferPlan,
    window: &gtk::Window,
) -> Option<Vec<(String, OnConflict)>> {
    let mut answers = Vec::new();
    let mut for_the_rest: Option<OnConflict> = None;
    for name in &plan.conflicts {
        if let Some(all) = for_the_rest {
            answers.push((name.clone(), all));
            continue;
        }
        let dialog = adw::AlertDialog::new(
            Some(&trf("files.already_there", &[("name", name)])),
            Some(&tr("files.already_there_hint")),
        );
        let rest = gtk::CheckButton::with_label(&tr("files.apply_to_rest"));
        if plan.conflicts.len() > 1 {
            dialog.set_extra_child(Some(&rest));
        }
        dialog.add_response("cancel", &tr("files.cancel"));
        dialog.add_response("skip", &tr("files.skip"));
        dialog.add_response("keep_both", &tr("files.keep_both"));
        dialog.add_response("replace", &tr("files.replace"));
        dialog.set_default_response(Some("keep_both"));
        dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);

        let choice = match ask(&dialog, window).await.as_str() {
            "skip" => OnConflict::Skip,
            "keep_both" => OnConflict::KeepBoth,
            "replace" => OnConflict::Replace,
            _ => return None,
        };
        if rest.is_active() {
            for_the_rest = Some(choice);
        }
        answers.push((name.clone(), choice));
    }
    Some(answers)
}

pub(crate) fn start_transfer(
    window: Option<gtk::Window>,
    cmd: UnboundedSender<ApiCmd>,
    dest: Side,
    from: TransferOrigin,
    into: Option<String>,
) {
    let Some(window) = window else { return };
    gtk::glib::spawn_future_local(async move {
        let (tx, rx) = oneshot::channel();
        if cmd
            .send(ApiCmd::FmTransferPlan {
                dest,
                from,
                into: into.clone(),
                reply: tx,
            })
            .is_err()
        {
            return;
        }
        let Ok(plan) = rx.await else { return };
        if plan.error.is_some() || plan.items.is_empty() {
            return;
        }

        let go = confirm_plan(&plan, &window).await;
        let resolutions = if go {
            resolve_conflicts(&plan, &window).await
        } else {
            None
        };
        match resolutions {
            Some(resolutions) => {
                let _ = cmd.send(ApiCmd::FmTransferRun {
                    dest,
                    from,
                    into,
                    resolutions,
                });
            }
            None if from == TransferOrigin::Clipboard => {
                let _ = cmd.send(ApiCmd::FmClipboardClear);
            }
            None => {}
        }
    });
}

pub(crate) struct ClipButtons {
    pub(crate) widget: gtk::Widget,
    side: Side,
    badge: gtk::Label,
    cut_slot: gtk::Overlay,
    copy_slot: gtk::Overlay,
    paste: gtk::Button,
    clear: gtk::Button,
}

fn icon_button(icon: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.set_child(Some(&icons::image(icon, 16)));
    b.add_css_class("flat");
    b.set_tooltip_text(Some(tip));
    b.set_focus_on_click(false);
    b
}

impl ClipButtons {
    pub(crate) fn new(
        side: Side,
        cmd: &UnboundedSender<ApiCmd>,
        arm: Rc<dyn Fn(TransferKind)>,
    ) -> Self {
        let cut = icon_button("cut", &tr("files.cut"));
        let copy = icon_button("copy", &tr("files.copy"));
        let paste = icon_button("paste", &tr("files.paste"));
        let clear = icon_button("cancel", &tr("files.clip_clear"));

        for (btn, kind) in [(&cut, TransferKind::Move), (&copy, TransferKind::Copy)] {
            let arm = arm.clone();
            btn.connect_clicked(move |_| arm(kind));
        }
        {
            let cmd = cmd.clone();
            paste.connect_clicked(move |b| {
                let window = b.root().and_downcast::<gtk::Window>();
                start_transfer(window, cmd.clone(), side, TransferOrigin::Clipboard, None);
            });
        }
        {
            let cmd = cmd.clone();
            clear.connect_clicked(move |_| {
                let _ = cmd.send(ApiCmd::FmClipboardClear);
            });
        }

        let badge = gtk::Label::new(None);
        badge.add_css_class("clip-badge");
        badge.set_halign(gtk::Align::End);
        badge.set_valign(gtk::Align::Start);
        badge.set_visible(false);
        let cut_slot = gtk::Overlay::new();
        cut_slot.set_child(Some(&cut));
        let copy_slot = gtk::Overlay::new();
        copy_slot.set_child(Some(&copy));

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        row.append(&cut_slot);
        row.append(&copy_slot);
        row.append(&paste);
        row.append(&clear);

        let me = Self {
            widget: row.upcast(),
            side,
            badge,
            cut_slot,
            copy_slot,
            paste,
            clear,
        };
        me.set(None);
        me
    }

    pub(crate) fn set(&self, view: Option<&ClipboardView>) {
        if let Some(parent) = self.badge.parent().and_downcast::<gtk::Overlay>() {
            parent.remove_overlay(&self.badge);
        }
        let held = view.filter(|v| v.count > 0);
        if let Some(v) = held.filter(|v| v.side == self.side) {
            let slot = match v.kind {
                TransferKind::Move => &self.cut_slot,
                TransferKind::Copy => &self.copy_slot,
            };
            slot.add_overlay(&self.badge);
            self.badge.set_text(&v.count.to_string());
            self.badge.set_visible(true);
        } else {
            self.badge.set_visible(false);
        }
        match held {
            Some(v) => {
                self.paste.set_sensitive(true);
                self.clear.set_sensitive(true);
                self.paste.set_tooltip_text(Some(&trf(
                    "files.paste_n",
                    &[("count", &v.count.to_string()), ("from", &v.source)],
                )));
            }
            None => {
                self.paste.set_sensitive(false);
                self.clear.set_sensitive(false);
                self.paste.set_tooltip_text(Some(&tr("files.paste")));
            }
        }
    }
}
