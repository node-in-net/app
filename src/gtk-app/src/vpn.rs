use adw::prelude::*;
use app_core::webvpn::VpnState;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use crate::i18n;
use crate::platform::VpnRequest;

const APPS_KEY: &str = "vpn.apps";
/// A start nothing answered by then was dropped on the way, a system dialog destroyed with it.
const PENDING_FOR: Duration = Duration::from_secs(60);

fn chosen(config: &client_config::AppConfig) -> Vec<String> {
    config.get::<Vec<String>>(APPS_KEY).unwrap_or_default()
}

fn save(config: &client_config::AppConfig, apps: Vec<String>) {
    config.set(APPS_KEY, apps);
    config.save();
}

/// The saved choice keeps missing apps: a disabled or archived app comes back under the same name.
fn present(chosen: Vec<String>, launcher: &[String]) -> Vec<String> {
    if launcher.is_empty() {
        return chosen;
    }
    chosen
        .into_iter()
        .filter(|app| launcher.contains(app))
        .collect()
}

fn sorted(mut apps: Vec<String>) -> Vec<String> {
    apps.sort();
    apps
}

struct Pending {
    generation: u64,
    peer: (String, String),
    since: Instant,
}

#[derive(Default)]
struct Shown {
    device: (String, String),
    usable: bool,
    names: HashMap<String, String>,
    state: VpnState,
    pending: Option<Pending>,
    /// A stop was sent and its answer has not come: a start asked now is answered one later.
    stopping: bool,
}

impl Shown {
    fn ask(&mut self, peer: (String, String)) {
        self.pending = Some(Pending {
            generation: self.state.generation + u64::from(self.stopping),
            peer,
            since: Instant::now(),
        });
    }

    fn pending_here(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.peer.0 == self.device.0)
    }

    fn target(&self) -> Option<(String, String)> {
        if let Some(pending) = &self.pending {
            return Some(pending.peer.clone());
        }
        let peer = self.state.peer.clone().filter(|_| self.state.running)?;
        let name = self.names.get(&peer).cloned().unwrap_or_default();
        Some((peer, name))
    }
}

struct Inner {
    label: gtk::Label,
    switch: gtk::Switch,
    apps: gtk::Button,
    status: gtk::Label,
    stop: gtk::Button,
    shown: RefCell<Shown>,
    syncing: Cell<bool>,
}

pub(crate) struct VpnSection {
    pub widget: gtk::Box,
    inner: Rc<Inner>,
}

impl VpnSection {
    pub fn new(config: &client_config::AppConfig) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 10);
        widget.set_margin_top(18);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let label = gtk::Label::new(None);
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        let switch = gtk::Switch::new();
        switch.set_valign(gtk::Align::Center);
        row.append(&label);
        row.append(&switch);
        let apps = gtk::Button::new();
        apps.add_css_class("pill");
        apps.set_halign(gtk::Align::Center);
        apps.set_label(&count_label(chosen(config).len()));
        let status = gtk::Label::new(None);
        status.set_wrap(true);
        status.set_justify(gtk::Justification::Center);
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        let stop = gtk::Button::with_label(&i18n::tr("vpn.stop"));
        stop.add_css_class("flat");
        stop.set_halign(gtk::Align::Center);
        stop.set_visible(false);
        widget.append(&row);
        widget.append(&apps);
        widget.append(&status);
        widget.append(&stop);

        let inner = Rc::new(Inner {
            label,
            switch,
            apps,
            status,
            stop,
            shown: RefCell::default(),
            syncing: Cell::new(false),
        });
        connect_switch(&inner, config);
        connect_apps(&inner, config);
        connect_stop(&inner);
        Self { widget, inner }
    }

    pub fn render(&self, id: &str, name: &str, usable: bool, names: HashMap<String, String>) {
        {
            let mut shown = self.inner.shown.borrow_mut();
            shown.device = (id.to_string(), name.to_string());
            shown.usable = usable;
            shown.names = names;
        }
        self.inner
            .label
            .set_text(&i18n::trf("vpn.route", &[("peer", name)]));
        self.inner.refresh();
    }

    pub fn apply(&self, state: VpnState) {
        {
            let mut shown = self.inner.shown.borrow_mut();
            // Events queued before a fresher reply arrive after it.
            if state.generation < shown.state.generation {
                return;
            }
            if state.generation > shown.state.generation {
                shown.stopping = false;
            }
            if shown
                .pending
                .as_ref()
                .is_some_and(|p| state.generation > p.generation)
            {
                shown.pending = None;
            }
            shown.state = state;
        }
        self.inner.refresh();
    }
}

impl Inner {
    fn asked(self: &Rc<Self>, peer: (String, String)) {
        self.shown.borrow_mut().ask(peer);
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local_once(PENDING_FOR, move || {
            if let Some(inner) = weak.upgrade() {
                inner.refresh();
            }
        });
        self.refresh();
    }

    fn refresh(&self) {
        let mut shown = self.shown.borrow_mut();
        if shown
            .pending
            .as_ref()
            .is_some_and(|p| p.since.elapsed() >= PENDING_FOR)
        {
            shown.pending = None;
        }
        let state = &shown.state;
        let ours = state.running && state.peer.as_deref() == Some(shown.device.0.as_str());
        let on = ours || shown.pending_here();
        // Setting even the same value ends GtkSwitch's toggle animation, and the user's tap with it.
        if self.switch.is_active() != on {
            self.syncing.set(true);
            self.switch.set_active(on);
            self.syncing.set(false);
        }
        self.switch.set_sensitive(on || shown.usable);
        self.stop
            .set_visible(state.running && !ours && shown.pending.is_none());
        if shown.pending_here() {
            self.status.set_text(&i18n::tr("vpn.starting"));
            return;
        }
        let name = |id: &Option<String>| {
            id.as_ref()
                .and_then(|id| shown.names.get(id).cloned())
                .unwrap_or_default()
        };
        let text = if ours && state.online {
            i18n::trf(
                "vpn.running",
                &[
                    ("count", &state.sockets.to_string()),
                    (
                        "traffic",
                        &gtk_net_ui::format_size(state.bytes_up + state.bytes_down),
                    ),
                ],
            )
        } else if ours {
            i18n::trf("vpn.peer_offline", &[("peer", &name(&state.peer))])
        } else if state.running {
            i18n::trf("vpn.elsewhere", &[("peer", &name(&state.peer))])
        } else {
            i18n::tr("vpn.note")
        };
        self.status.set_text(&text);
    }
}

fn connect_switch(inner: &Rc<Inner>, config: &client_config::AppConfig) {
    let config = config.clone();
    let weak = Rc::downgrade(inner);
    inner.switch.connect_active_notify(move |switch| {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        if inner.syncing.get() {
            return;
        }
        let Some(window) = switch.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let revert = |text: String| {
            inner.syncing.set(true);
            switch.set_active(!switch.is_active());
            inner.syncing.set(false);
            inner.status.set_text(&text);
        };
        if !switch.is_active() {
            let mut shown = inner.shown.borrow_mut();
            shown.pending = None;
            shown.stopping = true;
            drop(shown);
            crate::platform().stop_vpn(&window);
            return;
        }
        let apps = present(
            chosen(&config),
            &crate::platform().launcher_packages(&window),
        );
        inner.apps.set_label(&count_label(apps.len()));
        if apps.is_empty() {
            revert(i18n::tr("vpn.choose_first"));
            inner.apps.emit_clicked();
            return;
        }
        let device = inner.shown.borrow().device.clone();
        if start(&window, &apps, &device) {
            inner.asked(device);
        } else {
            revert(i18n::tr("vpn.failed"));
        }
    });
}

fn connect_stop(inner: &Rc<Inner>) {
    let weak = Rc::downgrade(inner);
    inner.stop.connect_clicked(move |btn| {
        let (Some(inner), Some(window)) =
            (weak.upgrade(), btn.root().and_downcast::<gtk::Window>())
        else {
            return;
        };
        inner.shown.borrow_mut().stopping = true;
        crate::platform().stop_vpn(&window);
    });
}

fn connect_apps(inner: &Rc<Inner>, config: &client_config::AppConfig) {
    let config = config.clone();
    let weak = Rc::downgrade(inner);
    inner.apps.connect_clicked(move |btn| {
        let Some(window) = btn.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let Some(nav) = btn
            .ancestor(adw::NavigationView::static_type())
            .and_downcast::<adw::NavigationView>()
        else {
            return;
        };
        let before = sorted(chosen(&config));
        let page = picker(crate::platform().launcher_apps(&window), &config);
        let config = config.clone();
        let weak: Weak<Inner> = weak.clone();
        page.connect_hidden(move |_| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let after = chosen(&config);
            inner.apps.set_label(&count_label(after.len()));
            if sorted(after.clone()) == before {
                return;
            }
            let Some(peer) = inner.shown.borrow().target() else {
                return;
            };
            let apps = present(after, &crate::platform().launcher_packages(&window));
            if apps.is_empty() {
                let mut shown = inner.shown.borrow_mut();
                shown.pending = None;
                shown.stopping = true;
                drop(shown);
                crate::platform().stop_vpn(&window);
            } else if start(&window, &apps, &peer) {
                inner.asked(peer);
            }
        });
        nav.push(&page);
    });
}

fn start(window: &gtk::Window, apps: &[String], device: &(String, String)) -> bool {
    let title = i18n::trf("vpn.notice", &[("peer", &device.1)]);
    let stop = i18n::tr("vpn.stop");
    crate::platform().start_vpn(
        window,
        &VpnRequest {
            apps,
            peer: &device.0,
            title: &title,
            stop: &stop,
        },
    )
}

fn count_label(count: usize) -> String {
    if count == 0 {
        i18n::tr("vpn.apps_none")
    } else {
        i18n::trf("vpn.apps_count", &[("count", &count.to_string())])
    }
}

fn picker(apps: Vec<(String, String)>, config: &client_config::AppConfig) -> adw::NavigationPage {
    let selected = chosen(config);
    let missing: Vec<(String, String)> = selected
        .iter()
        .filter(|app| !apps.iter().any(|(package, _)| package == *app))
        .map(|app| (app.clone(), app.clone()))
        .collect();
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    for (package, label) in apps.into_iter().chain(missing) {
        let row = adw::ActionRow::builder()
            .title(&label)
            .subtitle(&package)
            .use_markup(false)
            .build();
        let check = gtk::CheckButton::new();
        check.set_active(selected.contains(&package));
        row.add_prefix(&check);
        row.set_activatable_widget(Some(&check));
        let config = config.clone();
        check.connect_toggled(move |check| {
            let mut apps = chosen(&config);
            apps.retain(|p| p != &package);
            if check.is_active() {
                apps.push(package.clone());
            }
            save(&config, apps);
        });
        list.append(&row);
    }

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some(&i18n::tr("vpn.search")));
    {
        let list = list.clone();
        search.connect_search_changed(move |entry| {
            let needle = entry.text().to_lowercase();
            let mut index = 0;
            while let Some(row) = list.row_at_index(index) {
                let row_matches = row.downcast_ref::<adw::ActionRow>().is_some_and(|r| {
                    r.title().to_lowercase().contains(&needle)
                        || r.subtitle()
                            .is_some_and(|s| s.to_lowercase().contains(&needle))
                });
                row.set_visible(needle.is_empty() || row_matches);
                index += 1;
            }
        });
    }

    let col = gtk::Box::new(gtk::Orientation::Vertical, 12);
    col.set_margin_top(12);
    col.set_margin_start(12);
    col.set_margin_end(12);
    col.set_margin_bottom(24);
    col.append(&search);
    col.append(&list);
    let clamp = adw::Clamp::builder().maximum_size(600).child(&col).build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    adw::NavigationPage::with_tag(&scroll, &i18n::tr("vpn.apps_title"), "vpn-apps")
}
