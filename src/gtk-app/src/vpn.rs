use adw::prelude::*;
use app_core::webvpn::VpnState;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::i18n;
use crate::platform::VpnRequest;

const APPS_KEY: &str = "vpn.apps";
/// A start nothing answered by then was dropped on the way, a system dialog destroyed with it.
const PENDING_FOR: Duration = Duration::from_secs(60);
/// Each restart cuts the routed apps' connections: changes in a row restart the VPN once.
const SETTLE: Duration = Duration::from_millis(1200);
const ICON_SIZE: i32 = 32;

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
    config: client_config::AppConfig,
    group: adw::PreferencesGroup,
    switch: gtk::Switch,
    add: adw::ActionRow,
    rows: RefCell<Vec<adw::ActionRow>>,
    /// What the rows show, and whether the launcher had named them by then.
    listed: RefCell<Option<(Vec<String>, bool)>>,
    labels: RefCell<HashMap<String, String>>,
    labels_asked: Cell<bool>,
    icons: RefCell<HashMap<String, Option<gtk::gdk::Texture>>>,
    status: gtk::Label,
    stop: gtk::Button,
    shown: RefCell<Shown>,
    syncing: Cell<bool>,
    settle: Cell<u64>,
}

pub(crate) struct VpnSection {
    pub widget: gtk::Box,
    inner: Rc<Inner>,
}

impl VpnSection {
    pub fn new(config: &client_config::AppConfig) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 10);
        widget.set_margin_top(18);
        let switch = gtk::Switch::new();
        switch.set_valign(gtk::Align::Center);
        let group = adw::PreferencesGroup::new();
        group.set_header_suffix(Some(&switch));
        let add = adw::ActionRow::builder()
            .title(i18n::tr("vpn.add"))
            .activatable(true)
            .build();
        let plus = gtk::Image::from_icon_name("list-add-symbolic");
        plus.set_pixel_size(ICON_SIZE / 2);
        plus.set_size_request(ICON_SIZE, ICON_SIZE);
        add.add_prefix(&plus);
        group.add(&add);
        let status = gtk::Label::new(None);
        status.set_wrap(true);
        status.set_justify(gtk::Justification::Center);
        status.add_css_class("dim-label");
        status.add_css_class("caption");
        let stop = gtk::Button::with_label(&i18n::tr("vpn.stop"));
        stop.add_css_class("flat");
        stop.set_halign(gtk::Align::Center);
        stop.set_visible(false);
        widget.append(&group);
        widget.append(&status);
        widget.append(&stop);

        let inner = Rc::new(Inner {
            config: config.clone(),
            group,
            switch,
            add,
            rows: RefCell::default(),
            listed: RefCell::default(),
            labels: RefCell::default(),
            labels_asked: Cell::new(false),
            icons: RefCell::default(),
            status,
            stop,
            shown: RefCell::default(),
            syncing: Cell::new(false),
            settle: Cell::new(0),
        });
        connect_switch(&inner);
        connect_add(&inner);
        connect_stop(&inner);
        {
            let weak = Rc::downgrade(&inner);
            inner.group.connect_map(move |_| {
                if let Some(inner) = weak.upgrade() {
                    inner.sync_rows();
                }
            });
        }
        Self { widget, inner }
    }

    pub fn render(&self, id: &str, name: &str, usable: bool, names: HashMap<String, String>) {
        {
            let mut shown = self.inner.shown.borrow_mut();
            shown.device = (id.to_string(), name.to_string());
            shown.usable = usable;
            shown.names = names;
        }
        let peer = gtk::glib::markup_escape_text(name);
        self.inner
            .group
            .set_title(&i18n::trf("vpn.route", &[("peer", &peer)]));
        self.inner
            .group
            .set_description(Some(&i18n::trf("vpn.always", &[("peer", &peer)])));
        self.inner.sync_rows();
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
    fn window(&self) -> Option<gtk::Window> {
        self.group.root().and_downcast::<gtk::Window>()
    }

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

    /// The launcher's labels, asked for once: it loads every app's resources.
    fn learn_labels(&self, apps: Vec<(String, String)>) {
        self.labels_asked.set(true);
        self.labels.borrow_mut().extend(apps);
    }

    fn icon(&self, window: &gtk::Window, package: &str) -> Option<gtk::gdk::Texture> {
        if let Some(icon) = self.icons.borrow().get(package) {
            return icon.clone();
        }
        let size = ICON_SIZE * window.scale_factor().max(1);
        let icon = crate::platform()
            .app_icon(window, package, size)
            .and_then(|png| gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from_owned(png)).ok());
        self.icons
            .borrow_mut()
            .insert(package.to_string(), icon.clone());
        icon
    }

    fn sync_rows(self: &Rc<Self>) {
        let apps = chosen(&self.config);
        // Labels and icons cost the launcher's resources: only for the tab on screen.
        let window = self.window().filter(|_| self.group.is_mapped());
        if !self.labels_asked.get() && !apps.is_empty() {
            if let Some(window) = &window {
                self.learn_labels(crate::platform().launcher_apps(window));
            }
        }
        let named = self.labels_asked.get();
        if self.listed.borrow().as_ref() == Some(&(apps.clone(), named)) {
            return;
        }
        for row in self.rows.take() {
            self.group.remove(&row);
        }
        self.group.remove(&self.add);
        let mut rows = Vec::new();
        for package in &apps {
            let label = self
                .labels
                .borrow()
                .get(package)
                .cloned()
                .unwrap_or_else(|| package.clone());
            let row = adw::ActionRow::builder()
                .title(&label)
                .subtitle(package)
                .use_markup(false)
                .build();
            row.add_prefix(&app_image(
                window.as_ref().and_then(|w| self.icon(w, package)),
            ));
            let remove = gtk::Button::from_icon_name("list-remove-symbolic");
            remove.add_css_class("flat");
            remove.set_valign(gtk::Align::Center);
            remove.set_tooltip_text(Some(&i18n::tr("vpn.remove")));
            {
                let weak = Rc::downgrade(self);
                let package = package.clone();
                remove.connect_clicked(move |_| {
                    if let Some(inner) = weak.upgrade() {
                        inner.remove(&package);
                    }
                });
            }
            row.add_suffix(&remove);
            self.group.add(&row);
            rows.push(row);
        }
        self.group.add(&self.add);
        *self.rows.borrow_mut() = rows;
        *self.listed.borrow_mut() = Some((apps, named));
    }

    fn remove(self: &Rc<Self>, package: &str) {
        let mut apps = chosen(&self.config);
        apps.retain(|p| p != package);
        save(&self.config, apps);
        self.sync_rows();
        self.changed();
    }

    fn added(self: &Rc<Self>, package: String, then_start: bool) {
        let mut apps = chosen(&self.config);
        if !apps.contains(&package) {
            apps.push(package);
            save(&self.config, apps);
        }
        self.sync_rows();
        let usable = self.shown.borrow().usable;
        match self.window() {
            Some(window) if then_start && usable => {
                if let Err(text) = self.start_here(&window) {
                    self.status.set_text(&text);
                }
            }
            _ if then_start => self.refresh(),
            _ => self.changed(),
        }
    }

    /// A running VPN takes a new list only when it is re-established.
    fn changed(self: &Rc<Self>) {
        let ticket = self.settle.get() + 1;
        self.settle.set(ticket);
        let Some(window) = self.window() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let window = window.downgrade();
        gtk::glib::timeout_add_local_once(SETTLE, move || {
            let (Some(inner), Some(window)) = (weak.upgrade(), window.upgrade()) else {
                return;
            };
            if inner.settle.get() != ticket || inner.shown.borrow().stopping {
                return;
            }
            let Some(peer) = inner.shown.borrow().target() else {
                return;
            };
            let apps = present(
                chosen(&inner.config),
                &crate::platform().launcher_packages(&window),
            );
            if apps.is_empty() {
                inner.stopping();
                crate::platform().stop_vpn(&window);
            } else if start(&window, &apps, &peer) {
                inner.asked(peer);
            } else {
                inner.status.set_text(&i18n::tr("vpn.failed"));
            }
        });
    }

    fn cancel_restart(&self) {
        self.settle.set(self.settle.get() + 1);
    }

    fn stopping(&self) {
        self.cancel_restart();
        let mut shown = self.shown.borrow_mut();
        shown.pending = None;
        shown.stopping = true;
    }

    /// Err carries what to tell the user; with no app chosen yet, the picker opens.
    fn start_here(self: &Rc<Self>, window: &gtk::Window) -> Result<(), String> {
        self.cancel_restart();
        let apps = present(
            chosen(&self.config),
            &crate::platform().launcher_packages(window),
        );
        if apps.is_empty() {
            self.open_picker(true);
            return Err(i18n::tr("vpn.choose_first"));
        }
        let device = self.shown.borrow().device.clone();
        if start(window, &apps, &device) {
            self.asked(device);
            Ok(())
        } else {
            Err(i18n::tr("vpn.failed"))
        }
    }

    fn open_picker(self: &Rc<Self>, then_start: bool) {
        let Some(window) = self.window() else {
            return;
        };
        let Some(nav) = self
            .group
            .ancestor(adw::NavigationView::static_type())
            .and_downcast::<adw::NavigationView>()
        else {
            return;
        };
        let apps = crate::platform().launcher_apps(&window);
        self.learn_labels(apps.clone());
        let taken = chosen(&self.config);
        let apps = apps
            .into_iter()
            .filter(|(package, _)| !taken.contains(package))
            .collect();
        let weak = Rc::downgrade(self);
        let back = nav.clone();
        let focus = window.downgrade();
        let page = picker(apps, self, &window, move |package| {
            if let Some(window) = focus.upgrade() {
                gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
            }
            back.pop();
            if let Some(inner) = weak.upgrade() {
                inner.added(package, then_start);
            }
        });
        // A focused search field would raise the phone's keyboard over the list.
        gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
        nav.push(&page);
    }
}

fn app_image(icon: Option<gtk::gdk::Texture>) -> gtk::Image {
    let image = match icon {
        Some(texture) => gtk::Image::from_paintable(Some(&texture)),
        None => gtk::Image::from_icon_name("application-x-executable-symbolic"),
    };
    image.set_pixel_size(ICON_SIZE);
    image
}

fn connect_switch(inner: &Rc<Inner>) {
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
        if !switch.is_active() {
            inner.stopping();
            crate::platform().stop_vpn(&window);
            return;
        }
        if let Err(text) = inner.start_here(&window) {
            inner.syncing.set(true);
            switch.set_active(false);
            inner.syncing.set(false);
            inner.status.set_text(&text);
        }
    });
}

fn connect_add(inner: &Rc<Inner>) {
    let weak = Rc::downgrade(inner);
    inner.add.connect_activated(move |_| {
        if let Some(inner) = weak.upgrade() {
            inner.open_picker(false);
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
        inner.stopping();
        crate::platform().stop_vpn(&window);
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

fn picker(
    apps: Vec<(String, String)>,
    inner: &Rc<Inner>,
    window: &gtk::Window,
    pick: impl Fn(String) + 'static,
) -> adw::NavigationPage {
    let pick = Rc::new(pick);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    let mut images = Vec::new();
    for (package, label) in apps {
        let row = adw::ActionRow::builder()
            .title(&label)
            .subtitle(&package)
            .use_markup(false)
            .activatable(true)
            .build();
        let image = app_image(inner.icons.borrow().get(&package).cloned().flatten());
        row.add_prefix(&image);
        images.push((package.clone(), image));
        let pick = pick.clone();
        row.connect_activated(move |_| pick(package.clone()));
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
    let page = adw::NavigationPage::with_tag(&scroll, &i18n::tr("vpn.add"), "vpn-apps");

    // Icons come a few at a time, so that the list shows at once.
    let open = Rc::new(Cell::new(true));
    {
        let open = open.clone();
        page.connect_hidden(move |_| open.set(false));
    }
    let weak = Rc::downgrade(inner);
    let window = window.downgrade();
    let mut queue = images.into_iter();
    gtk::glib::idle_add_local(move || {
        let (Some(inner), Some(window)) = (weak.upgrade(), window.upgrade()) else {
            return gtk::glib::ControlFlow::Break;
        };
        if !open.get() {
            return gtk::glib::ControlFlow::Break;
        }
        for _ in 0..6 {
            let Some((package, image)) = queue.next() else {
                return gtk::glib::ControlFlow::Break;
            };
            if let Some(texture) = inner.icon(&window, &package) {
                image.set_paintable(Some(&texture));
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    page
}
