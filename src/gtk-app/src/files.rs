use adw::prelude::*;
use app_core::fm::{ClipboardView, PanelState, Side, TransferKind};
use app_headless::{ApiCmd, TransferOrigin};
use gtk_fm_ui::{
    BreadcrumbSegment, FmPanelInit, FmPanelInput, FmPanelModel, FmPanelOutput, RemoteFileEntry,
    SourceInfo,
};
use relm4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;

fn join(cwd: &str, name: &str) -> String {
    if cwd.ends_with('/') {
        format!("{cwd}{name}")
    } else {
        format!("{cwd}/{name}")
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[derive(Default)]
struct SideState {
    panel: PanelState,
    last_error: Option<String>,
    marked: Vec<String>,
    cursor: Option<String>,
}

pub struct FilesPane {
    pub widget: gtk::Widget,
    left: relm4::Controller<FmPanelModel>,
    right: relm4::Controller<FmPanelModel>,
    left_state: Rc<RefCell<SideState>>,
    right_state: Rc<RefCell<SideState>>,
    clips: [crate::clipboard::ClipButtons; 2],
}

type ActiveSide = Rc<Cell<Side>>;

impl FilesPane {
    pub fn new(cmd: UnboundedSender<ApiCmd>, config: client_config::AppConfig) -> Self {
        let left_state = Rc::new(RefCell::new(SideState::default()));
        let right_state = Rc::new(RefCell::new(SideState::default()));
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let window_of = {
            let root = root.clone();
            Rc::new(move || root.root().and_downcast::<gtk::Window>()) as Rc<dyn Fn() -> _>
        };
        let cmd_for_keys = cmd.clone();

        let left_clip = crate::clipboard::ClipButtons::new(
            Side::Left,
            &cmd,
            arm_fn(&cmd, Side::Left, &left_state),
        );
        let right_clip = crate::clipboard::ClipButtons::new(
            Side::Right,
            &cmd,
            arm_fn(&cmd, Side::Right, &right_state),
        );
        let left = build_panel(
            Side::Left,
            "fm-left",
            cmd.clone(),
            config.clone(),
            left_state.clone(),
            &left_clip,
            window_of.clone(),
        );
        let right = build_panel(
            Side::Right,
            "fm-right",
            cmd,
            config,
            right_state.clone(),
            &right_clip,
            window_of,
        );

        let active: ActiveSide = Rc::new(Cell::new(Side::Left));
        for w in [left.widget(), right.widget()] {
            w.add_css_class("fm-panel");
        }
        track_focus(left.widget(), Side::Left, &active, &cmd_for_keys);
        track_focus(right.widget(), Side::Right, &active, &cmd_for_keys);

        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(left.widget()));
        paned.set_end_child(Some(right.widget()));
        paned.set_resize_start_child(true);
        paned.set_resize_end_child(true);
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);
        paned.set_wide_handle(true);
        paned.set_vexpand(true);

        let states = (left_state.clone(), right_state.clone());
        let panels = (left.sender().clone(), right.sender().clone());
        root.append(&paned);
        let (bar, shift_labels) = fkey_bar(&cmd_for_keys, &active, &states);
        root.append(&bar);
        install_fkeys(
            &root,
            &cmd_for_keys,
            &active,
            &states,
            &panels,
            shift_labels,
        );

        Self {
            widget: root.upcast(),
            left,
            right,
            left_state,
            right_state,
            clips: [left_clip, right_clip],
        }
    }

    pub fn set_clipboard(&self, view: Option<&ClipboardView>) {
        for c in &self.clips {
            c.set(view);
        }
        let held = view.map(|v| v.count).unwrap_or(0);
        for ctl in [&self.left, &self.right] {
            let _ = ctl.sender().send(FmPanelInput::ClipboardHeld(held));
        }
    }

    pub fn apply(&self, side: Side, panel: PanelState) {
        let (ctl, state) = match side {
            Side::Left => (&self.left, &self.left_state),
            Side::Right => (&self.right, &self.right_state),
        };

        let new_error = panel.last_error.clone();
        let show_error = {
            let prev = &state.borrow().last_error;
            new_error.is_some() && new_error != *prev
        };
        if show_error {
            if let Some(msg) = &new_error {
                let _ = ctl.sender().send(FmPanelInput::OpFailed {
                    title: crate::i18n::tr("files.op_failed"),
                    message: msg.clone(),
                });
            }
        }

        let _ = ctl.sender().send(to_listing(side, &panel));
        {
            let mut st = state.borrow_mut();
            st.panel = panel;
            st.last_error = new_error;
        }
    }
}

type Sides = (Rc<RefCell<SideState>>, Rc<RefCell<SideState>>);
type Panels = (relm4::Sender<FmPanelInput>, relm4::Sender<FmPanelInput>);

#[derive(Clone, Copy, PartialEq)]
enum FKey {
    Rename,
    Copy,
    Move,
    Mkdir,
    Delete,
    View,
    Edit,
    NewFile,
}

struct FKeySpec {
    key: &'static str,
    label: &'static str,
    action: Option<FKey>,
    shift: Option<(&'static str, FKey)>,
}

const KEYS: &[FKeySpec] = &[
    FKeySpec {
        key: "F2",
        label: "files.rename",
        action: Some(FKey::Rename),
        shift: None,
    },
    FKeySpec {
        key: "F3",
        label: "files.view",
        action: Some(FKey::View),
        shift: None,
    },
    FKeySpec {
        key: "F4",
        label: "files.edit",
        action: Some(FKey::Edit),
        shift: Some(("files.new", FKey::NewFile)),
    },
    FKeySpec {
        key: "F5",
        label: "files.copy",
        action: Some(FKey::Copy),
        shift: None,
    },
    FKeySpec {
        key: "F6",
        label: "files.move",
        action: Some(FKey::Move),
        shift: None,
    },
    FKeySpec {
        key: "F7",
        label: "files.mkdir",
        action: Some(FKey::Mkdir),
        shift: None,
    },
    FKeySpec {
        key: "F8",
        label: "files.delete",
        action: Some(FKey::Delete),
        shift: None,
    },
];

#[derive(Clone)]
struct ShiftLabels {
    buttons: Rc<Vec<(gtk::Button, String, String)>>,
    held: Rc<Cell<bool>>,
}

impl ShiftLabels {
    fn apply(&self, shift_held: bool) {
        if self.held.replace(shift_held) == shift_held {
            return;
        }
        for (btn, plain, shifted) in self.buttons.iter() {
            btn.set_label(if shift_held { shifted } else { plain });
        }
    }
}

fn track_focus(
    widget: &impl IsA<gtk::Widget>,
    side: Side,
    active: &ActiveSide,
    cmd: &UnboundedSender<ApiCmd>,
) {
    let focus = gtk::EventControllerFocus::new();
    let active = active.clone();
    let cmd = cmd.clone();
    focus.connect_enter(move |_| {
        if active.get() != side {
            active.set(side);
            let _ = cmd.send(ApiCmd::FmActivate { side });
        }
    });
    widget.add_controller(focus);
}

fn fkey_bar(
    cmd: &UnboundedSender<ApiCmd>,
    active: &ActiveSide,
    states: &Sides,
) -> (gtk::Box, ShiftLabels) {
    let bar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .homogeneous(true)
        .spacing(4)
        .margin_start(6)
        .margin_end(6)
        .margin_top(2)
        .margin_bottom(6)
        .build();
    bar.add_css_class("fkey-bar");
    let shift_state = Rc::new(Cell::new(false));
    let mut shift_labels = Vec::new();
    for spec in KEYS {
        let plain = format!("{} {}", spec.key, crate::i18n::tr(spec.label));
        let btn = gtk::Button::with_label(&plain);
        btn.add_css_class("flat");
        btn.set_focus_on_click(false);
        btn.set_sensitive(spec.action.is_some());
        if let Some(action) = spec.action {
            let cmd = cmd.clone();
            let active = active.clone();
            let states = states.clone();
            let shift_action = spec.shift.map(|(_, a)| a);
            let held = shift_state.clone();
            btn.connect_clicked(move |b| {
                let window = b.root().and_downcast::<gtk::Window>();
                let chosen = match (held.get(), shift_action) {
                    (true, Some(a)) => a,
                    _ => action,
                };
                run(chosen, &cmd, &active, &states, window.as_ref());
            });
        }
        if let Some((shift_label, _)) = spec.shift {
            shift_labels.push((
                btn.clone(),
                plain,
                format!("{} {}", spec.key, crate::i18n::tr(shift_label)),
            ));
        }
        bar.append(&btn);
    }
    (
        bar,
        ShiftLabels {
            buttons: Rc::new(shift_labels),
            held: shift_state,
        },
    )
}

fn install_fkeys(
    root: &gtk::Box,
    cmd: &UnboundedSender<ApiCmd>,
    active: &ActiveSide,
    states: &Sides,
    panels: &Panels,
    shift_labels: ShiftLabels,
) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);

    {
        let labels = shift_labels.clone();
        keys.connect_key_released(move |_, keyval, _, _| {
            if matches!(keyval, gtk::gdk::Key::Shift_L | gtk::gdk::Key::Shift_R) {
                labels.apply(false);
            }
        });
    }
    {
        let labels = shift_labels.clone();
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave(move |_| labels.apply(false));
        root.add_controller(focus);
    }
    {
        let labels = shift_labels.clone();
        root.connect_realize(move |w| {
            if let Some(window) = w.root().and_downcast::<gtk::Window>() {
                let labels = labels.clone();
                window.connect_is_active_notify(move |win| {
                    if !win.is_active() {
                        labels.apply(false);
                    }
                });
            }
        });
    }

    let cmd = cmd.clone();
    let active = active.clone();
    let states = states.clone();
    let panels = panels.clone();
    keys.connect_key_pressed(move |ctl, keyval, _, state| {
        if matches!(keyval, gtk::gdk::Key::Shift_L | gtk::gdk::Key::Shift_R) {
            shift_labels.apply(true);
            return gtk::glib::Propagation::Proceed;
        }
        let window = ctl
            .widget()
            .and_then(|w| w.root())
            .and_downcast::<gtk::Window>();
        let typing = window
            .as_ref()
            .and_then(gtk::prelude::GtkWindowExt::focus)
            .map(|w| w.is::<gtk::Editable>())
            .unwrap_or(false);
        if matches!(keyval, gtk::gdk::Key::Tab | gtk::gdk::Key::ISO_Left_Tab) {
            if typing {
                return gtk::glib::Propagation::Proceed;
            }
            let (left, right) = &panels;
            let other = match active.get() {
                Side::Left => right,
                Side::Right => left,
            };
            let _ = other.send(FmPanelInput::GrabFocus);
            return gtk::glib::Propagation::Stop;
        }
        if state.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            if typing {
                return gtk::glib::Propagation::Proceed;
            }
            let side = active.get();
            let state = match side {
                Side::Left => &states.0,
                Side::Right => &states.1,
            };
            match keyval {
                gtk::gdk::Key::x | gtk::gdk::Key::X => {
                    arm_clipboard(&cmd, side, state, TransferKind::Move);
                }
                gtk::gdk::Key::c | gtk::gdk::Key::C => {
                    arm_clipboard(&cmd, side, state, TransferKind::Copy);
                }
                gtk::gdk::Key::v | gtk::gdk::Key::V => {
                    crate::clipboard::start_transfer(
                        window.clone(),
                        cmd.clone(),
                        side,
                        TransferOrigin::Clipboard,
                        None,
                    );
                }
                _ => return gtk::glib::Propagation::Proceed,
            }
            return gtk::glib::Propagation::Stop;
        }
        let shift = shift_labels.held.get() || state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let action = match keyval {
            gtk::gdk::Key::F2 => FKey::Rename,
            gtk::gdk::Key::F3 => FKey::View,
            gtk::gdk::Key::F4 if shift => FKey::NewFile,
            gtk::gdk::Key::F4 => FKey::Edit,
            gtk::gdk::Key::F5 => FKey::Copy,
            gtk::gdk::Key::F6 => FKey::Move,
            gtk::gdk::Key::F7 => FKey::Mkdir,
            gtk::gdk::Key::F8 => FKey::Delete,
            _ => return gtk::glib::Propagation::Proceed,
        };
        run(action, &cmd, &active, &states, window.as_ref());
        gtk::glib::Propagation::Stop
    });
    root.add_controller(keys);
}

fn targets(panel: &PanelState, marked: &[String], panel_cursor: Option<&str>) -> Vec<String> {
    let real: Vec<String> = marked.iter().filter(|n| *n != "..").cloned().collect();
    if !real.is_empty() {
        return real;
    }
    match panel_cursor {
        Some("..") => Vec::new(),
        Some(name) => vec![name.to_string()],
        None => panel
            .entries
            .get(panel.cursor)
            .map(|e| vec![e.name.clone()])
            .unwrap_or_default(),
    }
}

fn reconcile(
    cmd: &UnboundedSender<ApiCmd>,
    side: Side,
    panel: &PanelState,
    marked: &[String],
    panel_cursor: Option<&str>,
) {
    let idx_of = |n: &str| panel.entries.iter().position(|e| e.name == n);
    let want: Vec<usize> = marked.iter().filter_map(|n| idx_of(n)).collect();
    for i in &panel.selection {
        if !want.contains(i) {
            let _ = cmd.send(ApiCmd::FmToggleSelect { side, index: *i });
        }
    }
    for i in &want {
        if !panel.selection.contains(i) {
            let _ = cmd.send(ApiCmd::FmToggleSelect { side, index: *i });
        }
    }
    if let Some(i) = panel_cursor.and_then(idx_of) {
        if i != panel.cursor {
            let _ = cmd.send(ApiCmd::FmCursor { side, index: i });
        }
    }
}

fn arm_clipboard(
    cmd: &UnboundedSender<ApiCmd>,
    side: Side,
    state: &Rc<RefCell<SideState>>,
    kind: TransferKind,
) {
    let st = state.borrow();
    if targets(&st.panel, &st.marked, st.cursor.as_deref()).is_empty() {
        return;
    }
    reconcile(cmd, side, &st.panel, &st.marked, st.cursor.as_deref());
    let _ = cmd.send(ApiCmd::FmClipboardSet { side, kind });
}

fn arm_fn(
    cmd: &UnboundedSender<ApiCmd>,
    side: Side,
    state: &Rc<RefCell<SideState>>,
) -> Rc<dyn Fn(TransferKind)> {
    let cmd = cmd.clone();
    let state = state.clone();
    Rc::new(move |kind| arm_clipboard(&cmd, side, &state, kind))
}

fn run(
    action: FKey,
    cmd: &UnboundedSender<ApiCmd>,
    active: &ActiveSide,
    states: &Sides,
    window: Option<&gtk::Window>,
) {
    let side = active.get();
    let state = match side {
        Side::Left => &states.0,
        Side::Right => &states.1,
    };
    let (panel, marked, panel_cursor) = {
        let st = state.borrow();
        (st.panel.clone(), st.marked.clone(), st.cursor.clone())
    };
    let reconcile = || reconcile(cmd, side, &panel, &marked, panel_cursor.as_deref());
    match action {
        FKey::View | FKey::Edit => {
            let entry = panel_cursor
                .as_deref()
                .and_then(|name| panel.entries.iter().find(|e| e.name == name))
                .or_else(|| panel.entries.get(panel.cursor));
            let Some(entry) = entry else { return };
            if entry.is_dir {
                return;
            }
            let Some(window) = window else { return };
            crate::viewer::open_file(
                window,
                cmd.clone(),
                side,
                join(&panel.cwd, &entry.name),
                panel.cwd.clone(),
                action == FKey::Edit,
            );
        }
        FKey::NewFile => {
            let Some(window) = window else { return };
            crate::viewer::new_file(window, cmd.clone(), side, panel.cwd.clone());
        }
        FKey::Copy | FKey::Move => {
            if targets(&panel, &marked, panel_cursor.as_deref()).is_empty() {
                return;
            }
            reconcile();
            let dest = match side {
                Side::Left => Side::Right,
                Side::Right => Side::Left,
            };
            let kind = if action == FKey::Copy {
                TransferKind::Copy
            } else {
                TransferKind::Move
            };
            crate::clipboard::start_transfer(
                window.cloned(),
                cmd.clone(),
                dest,
                TransferOrigin::OtherPane(kind),
                None,
            );
        }
        FKey::Mkdir => {
            let cmd = cmd.clone();
            prompt(
                window,
                &crate::i18n::tr("files.new_folder"),
                &crate::i18n::tr("files.name"),
                "",
                move |name| {
                    let _ = cmd.send(ApiCmd::FmMkdir { side, name });
                },
            );
        }
        FKey::Rename => {
            let names = targets(&panel, &marked, panel_cursor.as_deref());
            let Some(old) = names.first().cloned() else {
                return;
            };
            let cmd = cmd.clone();
            prompt(
                window,
                &crate::i18n::tr("files.rename"),
                &crate::i18n::tr("files.new_name"),
                &old,
                move |new_name| {
                    let _ = cmd.send(ApiCmd::FmRename { side, new_name });
                },
            );
        }
        FKey::Delete => {
            let names = targets(&panel, &marked, panel_cursor.as_deref());
            if names.is_empty() {
                return;
            }
            reconcile();
            let body = if names.len() == 1 {
                crate::i18n::trf("files.delete_one", &[("name", &names[0])])
            } else {
                crate::i18n::trf("files.delete_many", &[("count", &names.len().to_string())])
            };
            let cmd = cmd.clone();
            confirm(window, &crate::i18n::tr("files.delete"), &body, move || {
                let _ = cmd.send(ApiCmd::FmDelete { side });
            });
        }
    }
}

fn prompt(
    window: Option<&gtk::Window>,
    title: &str,
    placeholder: &str,
    initial: &str,
    on_ok: impl Fn(String) + 'static,
) {
    let Some(window) = window else { return };
    let dialog = adw::AlertDialog::new(Some(title), None);
    let entry = gtk::Entry::builder()
        .placeholder_text(placeholder)
        .activates_default(true)
        .text(initial)
        .build();
    dialog.set_extra_child(Some(&entry));
    dialog.add_response("cancel", &crate::i18n::tr("files.cancel"));
    dialog.add_response("ok", title);
    dialog.set_default_response(Some("ok"));
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    dialog.connect_response(None, move |_, resp| {
        if resp == "ok" {
            let text = entry.text().trim().to_string();
            if !text.is_empty() {
                on_ok(text);
            }
        }
    });
    dialog.present(Some(window));
}

fn confirm(window: Option<&gtk::Window>, title: &str, body: &str, on_ok: impl Fn() + 'static) {
    let Some(window) = window else { return };
    let dialog = adw::AlertDialog::new(Some(title), Some(body));
    dialog.add_response("cancel", &crate::i18n::tr("files.cancel"));
    dialog.add_response("ok", title);
    dialog.set_default_response(Some("cancel"));
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Destructive);
    dialog.connect_response(None, move |_, resp| {
        if resp == "ok" {
            on_ok();
        }
    });
    dialog.present(Some(window));
}

fn build_panel(
    side: Side,
    panel_id: &str,
    cmd: UnboundedSender<ApiCmd>,
    config: client_config::AppConfig,
    state: Rc<RefCell<SideState>>,
    clip: &crate::clipboard::ClipButtons,
    window_of: Rc<dyn Fn() -> Option<gtk::Window>>,
) -> relm4::Controller<FmPanelModel> {
    FmPanelModel::builder()
        .launch(FmPanelInit {
            panel_id: panel_id.to_string(),
            show_toolbar: true,
            config,
            select_mask_enabled: false,
            thumbnailer: None,
            toolbar_start_extras: vec![clip.widget.clone()],
            toolbar_end_extras: Vec::new(),
        })
        .connect_receiver(move |_input, output| {
            if let FmPanelOutput::Clip { action, into } = output {
                clip_action(side, action, into.clone(), &cmd, &state, window_of());
                return;
            }
            translate(side, output, &cmd, &state);
        })
}

fn to_listing(side: Side, panel: &PanelState) -> FmPanelInput {
    let entries: Vec<RemoteFileEntry> = panel
        .entries
        .iter()
        .map(|e| RemoteFileEntry {
            name: e.name.clone(),
            is_dir: e.is_dir,
            size: e.size,
            modified: e.modified,
            permissions: e.permissions,
        })
        .collect();
    let breadcrumb: Vec<BreadcrumbSegment> = panel
        .breadcrumb
        .iter()
        .map(|c| BreadcrumbSegment {
            name: c.name.clone(),
            path: c.path.clone(),
            icon: "/com/fm-ui/gtk/folder.svg".to_string(),
        })
        .collect();
    let select_name = panel.entries.get(panel.cursor).map(|e| e.name.clone());
    let is_local = side == Side::Left;
    FmPanelInput::Listing {
        path: panel.cwd.clone(),
        entries,
        breadcrumb,
        source: SourceInfo {
            is_local,
            can_mount: !is_local,
            display_name: None,
            is_mounted: panel.mounted_as.is_some(),
            mount_name: panel
                .active_resource
                .and_then(|i| panel.resources.get(i))
                .map(|r| r.label.clone())
                .unwrap_or_default(),
            fs_label: Some(crate::i18n::tr(if is_local {
                "files.local_fs"
            } else {
                "files.remote_fs"
            })),
            root_icon: if is_local {
                "/com/fm-ui/gtk/home.svg".to_string()
            } else {
                "/com/fm-ui/gtk/netdrive.svg".to_string()
            },
            connection_id: None,
        },
        select_name,
    }
}

fn clip_action(
    side: Side,
    action: gtk_fm_ui::ClipAction,
    into: Option<String>,
    cmd: &UnboundedSender<ApiCmd>,
    state: &Rc<RefCell<SideState>>,
    window: Option<gtk::Window>,
) {
    match action {
        gtk_fm_ui::ClipAction::Cut => arm_clipboard(cmd, side, state, TransferKind::Move),
        gtk_fm_ui::ClipAction::Copy => arm_clipboard(cmd, side, state, TransferKind::Copy),
        gtk_fm_ui::ClipAction::Paste => {
            crate::clipboard::start_transfer(
                window,
                cmd.clone(),
                side,
                TransferOrigin::Clipboard,
                into,
            );
        }
    }
}

fn translate(
    side: Side,
    out: FmPanelOutput,
    cmd: &UnboundedSender<ApiCmd>,
    state: &Rc<RefCell<SideState>>,
) {
    let send = |c: ApiCmd| {
        let _ = cmd.send(c);
    };
    let (cwd, entries, selection) = {
        let s = state.borrow();
        (
            s.panel.cwd.clone(),
            s.panel.entries.clone(),
            s.panel.selection.clone(),
        )
    };
    let idx_of = |name: &str| entries.iter().position(|e| e.name == name);

    match out {
        FmPanelOutput::StateChanged {
            selected, cursor, ..
        } => {
            let mut st = state.borrow_mut();
            st.marked = selected.into_iter().map(|(name, _)| name).collect();
            st.cursor = cursor;
        }
        FmPanelOutput::ActivateFile { path } => {
            let parent = gtk::gio::Application::default()
                .and_downcast::<gtk::Application>()
                .and_then(|a| a.active_window());
            if let Some(parent) = parent {
                crate::viewer::open_file(&parent, cmd.clone(), side, path, cwd.clone(), false);
            }
        }
        FmPanelOutput::NavigateEnter(name) => send(ApiCmd::FmNavigate {
            side,
            path: join(&cwd, &name),
        }),
        FmPanelOutput::NavigateUp => send(ApiCmd::FmUp { side }),
        FmPanelOutput::NavigateLevel(0) => send(ApiCmd::FmNavigate {
            side,
            path: "/".to_string(),
        }),
        FmPanelOutput::NavigateLevel(k) => send(ApiCmd::FmBreadcrumb { side, index: k - 1 }),
        FmPanelOutput::Refresh => send(ApiCmd::FmRefresh { side }),
        FmPanelOutput::NavigateTyped(path) => send(ApiCmd::FmNavigate { side, path }),
        FmPanelOutput::Back => send(ApiCmd::FmUp { side }),
        FmPanelOutput::Start => send(ApiCmd::FmNavigate {
            side,
            path: "/".into(),
        }),
        FmPanelOutput::Download { paths } => {
            let target: Vec<usize> = paths.iter().filter_map(|p| idx_of(basename(p))).collect();
            for i in &selection {
                if !target.contains(i) {
                    send(ApiCmd::FmToggleSelect { side, index: *i });
                }
            }
            for i in &target {
                if !selection.contains(i) {
                    send(ApiCmd::FmToggleSelect { side, index: *i });
                }
            }
            send(ApiCmd::FmCopy {
                side: match side {
                    Side::Left => Side::Right,
                    Side::Right => Side::Left,
                },
            });
        }
        FmPanelOutput::Duplicate { src, dst } => send(ApiCmd::FmDuplicate {
            side,
            path: src,
            new_name: dst,
        }),
        FmPanelOutput::UploadFiles { dir, files } => send(ApiCmd::FmUpload { side, dir, files }),
        FmPanelOutput::HistoryBack => send(ApiCmd::FmBack { side }),
        FmPanelOutput::HistoryForward => send(ApiCmd::FmForward { side }),
        FmPanelOutput::Mkdir { name, .. } => send(ApiCmd::FmMkdir { side, name }),
        FmPanelOutput::Rename { old_path, new_path } => {
            if let Some(i) = idx_of(basename(&old_path)) {
                send(ApiCmd::FmCursor { side, index: i });
                send(ApiCmd::FmRename {
                    side,
                    new_name: basename(&new_path).to_string(),
                });
            }
        }
        FmPanelOutput::Delete { names, .. } => {
            let target: Vec<usize> = names.iter().filter_map(|n| idx_of(n)).collect();
            for i in &selection {
                if !target.contains(i) {
                    send(ApiCmd::FmToggleSelect { side, index: *i });
                }
            }
            for i in &target {
                if !selection.contains(i) {
                    send(ApiCmd::FmToggleSelect { side, index: *i });
                }
            }
            send(ApiCmd::FmDelete { side });
        }
        FmPanelOutput::Mount => {
            if state.borrow().panel.mounted_as.is_some() {
                send(ApiCmd::FmUnmount { side });
            } else {
                send(ApiCmd::FmMount { side });
            }
        }
        FmPanelOutput::Chmod { path, mode } => send(ApiCmd::FmChmod { side, path, mode }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::targets;
    use app_core::fm::{FileEntry, PanelState};

    fn panel(names: &[&str], cursor: usize) -> PanelState {
        PanelState {
            entries: names
                .iter()
                .map(|n| FileEntry {
                    name: (*n).to_string(),
                    is_dir: false,
                    size: 0,
                    modified: 0,
                    permissions: None,
                })
                .collect(),
            cursor,
            ..PanelState::default()
        }
    }

    #[test]
    fn highlighting_the_parent_link_selects_nothing() {
        let p = panel(&["a.txt", "b.txt"], 1);
        assert!(
            targets(&p, &[], Some("..")).is_empty(),
            "falling back to the core cursor here would act on b.txt, which the user \
             never pointed at",
        );
    }

    #[test]
    fn the_parent_link_is_dropped_from_a_wider_selection() {
        let p = panel(&["a.txt", "b.txt"], 0);
        assert_eq!(
            targets(&p, &["..".into(), "a.txt".into()], Some("..")),
            vec!["a.txt"]
        );
    }

    #[test]
    fn the_panel_cursor_wins_over_the_cores_slower_one() {
        let p = panel(&["a.txt", "b.txt"], 0);
        assert_eq!(targets(&p, &[], Some("b.txt")), vec!["b.txt"]);
    }

    #[test]
    fn without_a_panel_cursor_the_core_still_answers() {
        let p = panel(&["a.txt", "b.txt"], 1);
        assert_eq!(targets(&p, &[], None), vec!["b.txt"]);
    }
}
