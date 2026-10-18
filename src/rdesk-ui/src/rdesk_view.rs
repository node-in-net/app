use adw::prelude::*;
use gtk::glib;
use gtk::glib::translate::IntoGlib;
use nodeinnet_p2p::p2p::DesktopInputEvent;
use relm4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Once;

const BITRATES: [(u32, &str); 4] = [
    (400_000, "rdesk.quality_low"),
    (800_000, "rdesk.quality_standard"),
    (1_500_000, "rdesk.quality_high"),
    (3_000_000, "rdesk.quality_ultra"),
];

fn selected_bitrate(dd: &gtk::DropDown) -> u32 {
    BITRATES
        .get(dd.selected() as usize)
        .map(|(b, _)| *b)
        .unwrap_or(BITRATES[1].0)
}

static CSS_INIT: Once = Once::new();

fn init_css() {
    CSS_INIT.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            "
            .rdesk-status-inactive { color: #f87171; font-weight: bold; font-family: monospace; }
            .rdesk-status-active { color: #34d399; font-weight: bold; font-family: monospace; text-shadow: 0 0 8px rgba(52, 211, 153, 0.4); }

            .rdesk-canvas {
                border: 2px solid #2d3748;
                border-radius: 12px;
                background: #090d16;
                box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
            }

            .rdesk-glass-card {
                background-color: alpha(@theme_bg_color, 0.95);
                border: 1px solid alpha(@theme_fg_color, 0.1);
                border-radius: 16px;
                padding: 40px;
                box-shadow: 0 15px 35px rgba(0, 0, 0, 0.1);
            }

            .rdesk-diag-card {
                background: rgba(13, 20, 38, 0.95);
                border: 1px solid rgba(0, 204, 255, 0.15);
                border-radius: 12px;
                padding: 16px 24px;
                margin-top: 16px;
                box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
            }
            .rdesk-diag-label {
                font-family: monospace;
                font-size: 13px;
            }
        ",
        );
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("Could not connect to a display."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

/// Zoom over the fitted frame (1 = fit) and how far it is panned. Touch only; the desktop
/// always draws at fit.
#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    zoom: f64,
    pan_x: f64,
    pan_y: f64,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

/// The frame's scale and top-left corner in the canvas. A frame larger than the canvas is
/// panned, and never past its edges.
fn placement((da_w, da_h): (f64, f64), (f_w, f_h): (f64, f64), view: View) -> (f64, f64, f64) {
    let scale = (da_w / f_w).min(da_h / f_h) * view.zoom;
    let place = |da: f64, img: f64, pan: f64| {
        if img <= da {
            (da - img) / 2.0
        } else {
            ((da - img) / 2.0 + pan).clamp(da - img, 0.0)
        }
    };
    (
        scale,
        place(da_w, f_w * scale, view.pan_x),
        place(da_h, f_h * scale, view.pan_y),
    )
}

/// Keeps the pan within what `placement` can show, so a pan past an edge does not pile up.
fn clamp_view(da: (f64, f64), frame: (f64, f64), view: View) -> View {
    let (_, x, y) = placement(da, frame, view);
    let centered = placement(
        da,
        frame,
        View {
            zoom: view.zoom,
            pan_x: 0.0,
            pan_y: 0.0,
        },
    );
    View {
        zoom: view.zoom,
        pan_x: x - centered.1,
        pan_y: y - centered.2,
    }
}

#[cfg(test)]
fn get_translated_coords(
    da: (f64, f64),
    widget: (f64, f64),
    frame: (f64, f64),
    screen: (f64, f64),
) -> Option<(i32, i32)> {
    translated_coords(da, widget, frame, screen, View::default())
}

fn translated_coords(
    (da_width, da_height): (f64, f64),
    (x_widget, y_widget): (f64, f64),
    (f_w, f_h): (f64, f64),
    (scr_w, scr_h): (f64, f64),
    view: View,
) -> Option<(i32, i32)> {
    if f_w <= 0.0 || f_h <= 0.0 {
        return None;
    }

    let (scale, offset_x, offset_y) = placement((da_width, da_height), (f_w, f_h), view);

    let x_frame = (x_widget - offset_x) / scale;
    let y_frame = (y_widget - offset_y) / scale;

    if x_frame >= 0.0 && x_frame < f_w && y_frame >= 0.0 && y_frame < f_h {
        let x_remote = (x_frame / f_w) * scr_w;
        let y_remote = (y_frame / f_h) * scr_h;
        Some((x_remote as i32, y_remote as i32))
    } else {
        None
    }
}

/// Touch on the remote screen: two fingers pinch-zoom, one finger pans a zoomed frame, and a
/// long press is a right click. A tap is the click gesture's.
fn add_touch_gestures(
    da: &gtk::DrawingArea,
    view: &Rc<Cell<View>>,
    draw_state: &Rc<RefCell<DrawState>>,
    mirror: &Rc<Cell<LiveMirror>>,
    geom: &GeomMirror,
    sender: &ComponentSender<RemoteDesktopModel>,
) {
    let sizes = {
        let da = da.clone();
        let draw_state = draw_state.clone();
        Rc::new(move || {
            let ds = draw_state.borrow();
            (
                (da.width() as f64, da.height() as f64),
                (ds.width as f64, ds.height as f64),
            )
        })
    };

    let zoom = gtk::GestureZoom::new();
    let start: Rc<Cell<(View, f64, f64)>> = Rc::new(Cell::new((View::default(), 0.0, 0.0)));
    {
        let start = start.clone();
        let view = view.clone();
        zoom.connect_begin(move |g, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            if let Some((x, y)) = g.bounding_box_center() {
                start.set((view.get(), x, y));
            }
        });
    }
    {
        let view = view.clone();
        let sizes = sizes.clone();
        let da = da.clone();
        zoom.connect_scale_changed(move |g, scale| {
            let Some((cx, cy)) = g.bounding_box_center() else {
                return;
            };
            let (v0, cx0, cy0) = start.get();
            let (size, frame) = sizes();
            if frame.0 <= 0.0 || frame.1 <= 0.0 {
                return;
            }
            // The frame point under the fingers stays under them.
            let (s0, ox0, oy0) = placement(size, frame, v0);
            let (fx, fy) = ((cx0 - ox0) / s0, (cy0 - oy0) / s0);
            let zoom = (v0.zoom * scale).clamp(1.0, 6.0);
            let (s1, cx1, cy1) = placement(
                size,
                frame,
                View {
                    zoom,
                    pan_x: 0.0,
                    pan_y: 0.0,
                },
            );
            view.set(clamp_view(
                size,
                frame,
                View {
                    zoom,
                    pan_x: cx - fx * s1 - cx1,
                    pan_y: cy - fy * s1 - cy1,
                },
            ));
            da.queue_draw();
        });
    }
    da.add_controller(zoom);

    let drag = gtk::GestureDrag::new();
    drag.set_touch_only(true);
    let origin = Rc::new(Cell::new(View::default()));
    {
        let origin = origin.clone();
        let view = view.clone();
        drag.connect_drag_begin(move |_, _, _| origin.set(view.get()));
    }
    {
        let view = view.clone();
        let da = da.clone();
        drag.connect_drag_update(move |g, dx, dy| {
            let v0 = origin.get();
            if v0.zoom <= 1.0 || dx.hypot(dy) < 8.0 {
                return;
            }
            g.set_state(gtk::EventSequenceState::Claimed);
            let (size, frame) = sizes();
            view.set(clamp_view(
                size,
                frame,
                View {
                    pan_x: v0.pan_x + dx,
                    pan_y: v0.pan_y + dy,
                    ..v0
                },
            ));
            da.queue_draw();
        });
    }
    da.add_controller(drag);

    let press = gtk::GestureLongPress::new();
    press.set_touch_only(true);
    {
        let view = view.clone();
        let mirror = mirror.clone();
        let geom = geom.clone();
        let da = da.clone();
        let sender = sender.clone();
        press.connect_pressed(move |g, x, y| {
            if !mirror.get().capturing() {
                return;
            }
            g.set_state(gtk::EventSequenceState::Claimed);
            if let Some((rx, ry)) = translate_pointer(&da, &geom, view.get(), x, y) {
                let button = remap_button(3);
                for event in [
                    DesktopInputEvent::MouseMove { x: rx, y: ry },
                    DesktopInputEvent::MouseDown { button },
                    DesktopInputEvent::MouseUp { button },
                ] {
                    let _ = sender.output(RemoteDesktopOutput::InputEvent(event));
                }
            }
        });
    }
    da.add_controller(press);
}

fn remap_button(gtk_btn: u32) -> u8 {
    match gtk_btn {
        1 => 1,
        3 => 2,
        2 => 3,
        _ => gtk_btn as u8,
    }
}

type GeomMirror = Rc<Cell<(usize, usize, usize, usize)>>;

fn translate_pointer(
    da: &gtk::DrawingArea,
    geom: &GeomMirror,
    view: View,
    x: f64,
    y: f64,
) -> Option<(i32, i32)> {
    let (f_w, f_h, scr_w, scr_h) = geom.get();
    translated_coords(
        (da.width() as f64, da.height() as f64),
        (x, y),
        (f_w as f64, f_h as f64),
        (scr_w as f64, scr_h as f64),
        view,
    )
}

#[derive(Clone, Copy)]
struct LiveMirror {
    connected: bool,
    controlling: bool,
    local: bool,
}

impl LiveMirror {
    fn capturing(&self) -> bool {
        self.connected && self.controlling && !self.local
    }
}

struct DrawState {
    connected: bool,
    frame_received: bool,
    width: usize,
    height: usize,
    bgra: Vec<u8>,
}

pub struct RemoteDesktopInit {
    pub show_toolbar: bool,
}

#[derive(Debug)]
pub enum RemoteDesktopInput {
    SetSession {
        resource_id: String,
        local: bool,
    },
    ConnectionChanged {
        connected: bool,
        remote_w: usize,
        remote_h: usize,
    },
    ConnectionError(String),
    ControlChanged(bool),
    Frame {
        width: usize,
        height: usize,
        bgra: Vec<u8>,
        compressed_len: usize,
    },
    StatsTick,
    ScreenSize {
        width: usize,
        height: usize,
    },
    SetOriginalSize(bool),
    SetBitrate(u32),
    SetForceSelect(bool),
}

#[derive(Debug)]
pub enum RemoteDesktopOutput {
    Connect {
        connect: bool,
        original_size: bool,
        bitrate_bps: Option<u32>,
        force_select: bool,
    },
    SetControl(bool),
    InputEvent(DesktopInputEvent),
    Back,
}

pub struct RemoteDesktopModel {
    show_toolbar: bool,
    resource_id: String,
    local: bool,
    connected: bool,
    controlling: bool,
    original_size: bool,
    bitrate_bps: u32,
    force_select: bool,
    remote_w: usize,
    remote_h: usize,
    error: Option<String>,
    stats_text: String,
    window_bytes: usize,
    window_frames: usize,
    frame_generation: u64,
    draw_state: Rc<RefCell<DrawState>>,
    mirror: Rc<Cell<LiveMirror>>,
    geom_mirror: GeomMirror,
    stats_gate: Rc<Cell<bool>>,
    stats_timer_armed: Rc<Cell<bool>>,
}

impl RemoteDesktopModel {
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    fn arm_stats_timer_if_needed(&self, sender: &ComponentSender<Self>) {
        if self.stats_timer_armed.get() {
            return;
        }
        self.stats_timer_armed.set(true);
        let gate = self.stats_gate.clone();
        let armed = self.stats_timer_armed.clone();
        let sender = sender.clone();
        glib::timeout_add_seconds_local(1, move || {
            if !gate.get() {
                armed.set(false);
                return glib::ControlFlow::Break;
            }
            sender.input(RemoteDesktopInput::StatsTick);
            glib::ControlFlow::Continue
        });
    }

    fn sync_mirrors(&self) {
        self.mirror.set(LiveMirror {
            connected: self.connected,
            controlling: self.controlling,
            local: self.local,
        });
        let mut ds = self.draw_state.borrow_mut();
        ds.connected = self.connected || self.local;
        self.geom_mirror
            .set((ds.width, ds.height, self.remote_w, self.remote_h));
    }
}

pub struct RemoteDesktopWidgets {
    header: adw::HeaderBar,
    status_lbl: gtk::Label,
    connect_btn: gtk::Button,
    control_switch: gtk::Switch,
    control_handler: glib::SignalHandlerId,
    original_size_switch: gtk::Switch,
    original_size_handler: glib::SignalHandlerId,
    force_select_switch: gtk::Switch,
    force_select_handler: glib::SignalHandlerId,
    stack: gtk::Stack,
    drawing_area: gtk::DrawingArea,
    rendered_frame_generation: u64,
    rendered_presentation: (bool, bool),
}

impl SimpleComponent for RemoteDesktopModel {
    type Init = RemoteDesktopInit;
    type Input = RemoteDesktopInput;
    type Output = RemoteDesktopOutput;
    type Root = gtk::Box;
    type Widgets = RemoteDesktopWidgets;

    fn init_root() -> Self::Root {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_vexpand(true);
        root.set_hexpand(true);
        root
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        crate::init_resources();
        init_css();

        let empty_title = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .title_widget(&empty_title)
            // Inside the phone's navigation view it would grow a second back button.
            .show_back_button(false)
            .build();

        let back_btn = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("go-previous-symbolic"))
            .css_classes(vec!["flat", "circular"])
            .tooltip_text(&*crate::i18n::tr("rdesk.back_to_devices"))
            .build();
        back_btn.set_cursor_from_name(Some("pointer"));
        let sender_back = sender.clone();
        back_btn.connect_clicked(move |_| {
            let _ = sender_back.output(RemoteDesktopOutput::Back);
        });
        let touch = gtk_dialogs::touch();
        if !touch {
            header.pack_start(&back_btn);
            header.pack_start(
                &gtk::Separator::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .margin_top(8)
                    .margin_bottom(8)
                    .build(),
            );
        }

        let control_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        control_box.set_valign(gtk::Align::Center);
        let control_lbl = gtk::Label::builder()
            .label(&*crate::i18n::tr("rdesk.remote_control"))
            .css_classes(vec!["dim-label"])
            .build();
        let control_switch = gtk::Switch::builder()
            .active(true)
            .valign(gtk::Align::Center)
            .margin_start(4)
            .build();
        control_switch.set_cursor_from_name(Some("pointer"));
        control_box.append(&control_lbl);
        control_box.append(&control_switch);
        header.pack_start(&control_box);

        let size_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        size_box.set_valign(gtk::Align::Center);
        size_box.set_margin_start(16);
        let size_lbl = gtk::Label::builder()
            .label(&*crate::i18n::tr("rdesk.original_size"))
            .css_classes(vec!["dim-label"])
            .build();
        let original_size_switch = gtk::Switch::builder()
            .active(false)
            .valign(gtk::Align::Center)
            .margin_start(4)
            .build();
        original_size_switch.set_cursor_from_name(Some("pointer"));
        size_box.append(&size_lbl);
        size_box.append(&original_size_switch);

        let quality_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        quality_box.set_valign(gtk::Align::Center);
        quality_box.set_margin_start(16);
        let quality_lbl = gtk::Label::builder()
            .label(&*crate::i18n::tr("rdesk.quality"))
            .css_classes(vec!["dim-label"])
            .build();
        let quality_model = gtk::StringList::new(
            &BITRATES
                .iter()
                .map(|(_, key)| crate::i18n::tr(key))
                .collect::<Vec<_>>()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        );
        let quality_combo = gtk::DropDown::builder()
            .model(&quality_model)
            .selected(1)
            .valign(gtk::Align::Center)
            .build();
        quality_box.append(&quality_lbl);
        quality_box.append(&quality_combo);

        let select_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        select_box.set_valign(gtk::Align::Center);
        select_box.set_margin_start(16);
        let select_lbl = gtk::Label::builder()
            .label(&*crate::i18n::tr("rdesk.select_screen"))
            .css_classes(vec!["dim-label"])
            .build();
        let force_select_switch = gtk::Switch::builder()
            .active(false)
            .valign(gtk::Align::Center)
            .margin_start(4)
            .build();
        force_select_switch.set_cursor_from_name(Some("pointer"));
        select_box.append(&select_lbl);
        select_box.append(&force_select_switch);
        let view: Rc<Cell<View>> = Rc::new(Cell::new(View::default()));
        // Touch: fit the whole screen, or fill the height and pan sideways to tap precisely.
        let fill_btn = gtk::ToggleButton::builder()
            .child(&gtk::Image::from_resource(
                "/com/rdesk-ui/gtk/fill-height.svg",
            ))
            .css_classes(vec!["flat", "circular"])
            .valign(gtk::Align::Center)
            .build();
        if touch {
            // A phone keeps Control and Connect in the bar; the rest opens in a sheet.
            let sheet_box = gtk::Box::new(gtk::Orientation::Vertical, 18);
            sheet_box.set_margin_top(24);
            sheet_box.set_margin_bottom(24);
            sheet_box.set_margin_start(24);
            sheet_box.set_margin_end(24);
            for b in [&size_box, &quality_box, &select_box] {
                b.set_margin_start(0);
                b.set_halign(gtk::Align::Center);
                sheet_box.append(b);
            }
            let sheet = adw::Dialog::builder()
                .presentation_mode(adw::DialogPresentationMode::BottomSheet)
                .child(&sheet_box)
                .build();
            let settings_btn = gtk::Button::builder()
                .child(&gtk::Image::from_icon_name("emblem-system-symbolic"))
                .css_classes(vec!["flat", "circular"])
                .valign(gtk::Align::Center)
                .build();
            settings_btn.connect_clicked(move |b| sheet.present(Some(b)));
            header.pack_end(&settings_btn);
            fill_btn.set_tooltip_text(Some(&crate::i18n::tr("rdesk.fill_height")));
            header.pack_end(&fill_btn);
        } else {
            header.pack_start(&size_box);
            header.pack_start(&quality_box);
            header.pack_start(&select_box);
        }

        let connect_btn = gtk::Button::builder()
            .label(crate::i18n::tr("rdesk.connect_session"))
            .css_classes(vec!["suggested-action"])
            .valign(gtk::Align::Center)
            .build();
        connect_btn.set_cursor_from_name(Some("pointer"));
        header.pack_end(&connect_btn);

        let status_lbl = gtk::Label::builder()
            .label(&*crate::i18n::tr("rdesk.disconnected"))
            .css_classes(vec!["rdesk-status-inactive"])
            .margin_end(12)
            .valign(gtk::Align::Center)
            .build();
        if touch {
            status_lbl.set_visible(false);
        } else {
            header.pack_end(&status_lbl);
        }

        header.set_visible(init.show_toolbar);
        root.append(&header);

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vexpand(true)
            .hexpand(true)
            .build();
        root.append(&stack);

        let dashboard_wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
        dashboard_wrapper.set_valign(gtk::Align::Center);
        dashboard_wrapper.set_halign(gtk::Align::Center);

        let glass_card = gtk::Box::new(gtk::Orientation::Vertical, 24);
        glass_card.add_css_class("rdesk-glass-card");
        if !touch {
            glass_card.set_width_request(450);
        }

        let dash_icon = gtk::Image::from_resource("/com/rdesk-ui/gtk/desktop-pc.svg");
        dash_icon.set_pixel_size(80);
        dash_icon.set_halign(gtk::Align::Center);

        let dash_title = gtk::Label::builder()
            .label(crate::i18n::tr("rdesk.establish_title"))
            .css_classes(vec!["title-2"])
            .halign(gtk::Align::Center)
            .build();

        let dash_desc = gtk::Label::builder()
            .label(crate::i18n::tr("rdesk.establish_desc"))
            .justify(gtk::Justification::Center)
            .css_classes(vec!["dim-label"])
            .wrap(true)
            .halign(gtk::Align::Center)
            .build();

        let dash_connect_btn = gtk::Button::builder()
            .label(crate::i18n::tr("rdesk.start_session"))
            .css_classes(vec!["rdesk-glow-btn", "pill"])
            .margin_top(16)
            .build();
        dash_connect_btn.set_cursor_from_name(Some("pointer"));

        glass_card.append(&dash_icon);
        glass_card.append(&dash_title);
        glass_card.append(&dash_desc);
        glass_card.append(&dash_connect_btn);
        dashboard_wrapper.append(&glass_card);
        stack.add_named(&dashboard_wrapper, Some("dashboard"));

        let canvas_wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let margin = if touch { 4 } else { 24 };
        canvas_wrapper.set_margin_start(margin);
        canvas_wrapper.set_margin_end(margin);
        canvas_wrapper.set_margin_top(margin);
        canvas_wrapper.set_margin_bottom(margin);

        let drawing_area = gtk::DrawingArea::new();
        drawing_area.add_css_class("rdesk-canvas");
        drawing_area.set_vexpand(true);
        drawing_area.set_hexpand(true);
        drawing_area.set_focusable(true);
        canvas_wrapper.append(&drawing_area);

        stack.add_named(&canvas_wrapper, Some("session"));

        let draw_state = Rc::new(RefCell::new(DrawState {
            connected: false,
            frame_received: false,
            width: 640,
            height: 480,
            bgra: vec![0u8; 640 * 480 * 4],
        }));
        let mirror = Rc::new(Cell::new(LiveMirror {
            connected: false,
            controlling: true,
            local: false,
        }));
        let geom_mirror: GeomMirror = Rc::new(Cell::new((640, 480, 1280, 720)));

        let motion_controller = gtk::EventControllerMotion::new();
        {
            let mirror = mirror.clone();
            let geom = geom_mirror.clone();
            let view = view.clone();
            let da = drawing_area.clone();
            let sender = sender.clone();
            motion_controller.connect_motion(move |_, x, y| {
                if !mirror.get().capturing() {
                    return;
                }
                if let Some((rx, ry)) = translate_pointer(&da, &geom, view.get(), x, y) {
                    let _ = sender.output(RemoteDesktopOutput::InputEvent(
                        DesktopInputEvent::MouseMove { x: rx, y: ry },
                    ));
                }
            });
        }
        drawing_area.add_controller(motion_controller);

        let click_gesture = gtk::GestureClick::new();
        click_gesture.set_button(0);
        {
            let mirror = mirror.clone();
            let geom = geom_mirror.clone();
            let view = view.clone();
            let da = drawing_area.clone();
            let sender = sender.clone();
            click_gesture.connect_pressed(move |gesture, _, x, y| {
                da.grab_focus();
                // A finger clicks on release: pressing first would hold the button down for
                // the whole pan or pinch that may follow.
                if touch || !mirror.get().capturing() {
                    return;
                }
                if let Some((rx, ry)) = translate_pointer(&da, &geom, view.get(), x, y) {
                    let button = remap_button(gesture.current_button());
                    let _ = sender.output(RemoteDesktopOutput::InputEvent(
                        DesktopInputEvent::MouseMove { x: rx, y: ry },
                    ));
                    let _ = sender.output(RemoteDesktopOutput::InputEvent(
                        DesktopInputEvent::MouseDown { button },
                    ));
                }
            });
        }
        {
            let mirror = mirror.clone();
            let geom = geom_mirror.clone();
            let view = view.clone();
            let da = drawing_area.clone();
            let sender = sender.clone();
            click_gesture.connect_released(move |gesture, _, x, y| {
                if !mirror.get().capturing() {
                    return;
                }
                if let Some((rx, ry)) = translate_pointer(&da, &geom, view.get(), x, y) {
                    let button = remap_button(gesture.current_button());
                    let _ = sender.output(RemoteDesktopOutput::InputEvent(
                        DesktopInputEvent::MouseMove { x: rx, y: ry },
                    ));
                    if touch {
                        let _ = sender.output(RemoteDesktopOutput::InputEvent(
                            DesktopInputEvent::MouseDown { button },
                        ));
                    }
                    let _ = sender.output(RemoteDesktopOutput::InputEvent(
                        DesktopInputEvent::MouseUp { button },
                    ));
                }
            });
        }
        drawing_area.add_controller(click_gesture);
        if touch {
            add_touch_gestures(
                &drawing_area,
                &view,
                &draw_state,
                &mirror,
                &geom_mirror,
                &sender,
            );
            let da = drawing_area.clone();
            let view = view.clone();
            let draw_state = draw_state.clone();
            fill_btn.connect_toggled(move |b| {
                let size = (da.width() as f64, da.height() as f64);
                let frame = {
                    let ds = draw_state.borrow();
                    (ds.width as f64, ds.height as f64)
                };
                let zoom = if b.is_active() && frame.0 > 0.0 && frame.1 > 0.0 {
                    let fit = (size.0 / frame.0).min(size.1 / frame.1);
                    (size.1 / frame.1 / fit).max(1.0)
                } else {
                    1.0
                };
                view.set(View {
                    zoom,
                    ..View::default()
                });
                da.queue_draw();
            });
        }

        let scroll_controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        {
            let mirror = mirror.clone();
            let sender = sender.clone();
            scroll_controller.connect_scroll(move |_, _, dy| {
                if !mirror.get().capturing() {
                    return glib::Propagation::Proceed;
                }
                let dy_val = -dy;
                let dy_int = if dy_val > 0.0 { 1 } else { -1 };
                let _ = sender.output(RemoteDesktopOutput::InputEvent(
                    DesktopInputEvent::MouseScroll { dy: dy_int },
                ));
                glib::Propagation::Stop
            });
        }
        drawing_area.add_controller(scroll_controller);

        let key_controller = gtk::EventControllerKey::new();
        {
            let mirror = mirror.clone();
            let sender = sender.clone();
            key_controller.connect_key_pressed(move |_, keyval, _keycode, _state| {
                if !mirror.get().capturing() {
                    return glib::Propagation::Proceed;
                }
                let _ = sender.output(RemoteDesktopOutput::InputEvent(
                    DesktopInputEvent::KeyPress {
                        keyval: keyval.into_glib(),
                    },
                ));
                glib::Propagation::Stop
            });
        }
        {
            let mirror = mirror.clone();
            let sender = sender.clone();
            key_controller.connect_key_released(move |_, keyval, _keycode, _state| {
                if !mirror.get().capturing() {
                    return;
                }
                let _ = sender.output(RemoteDesktopOutput::InputEvent(
                    DesktopInputEvent::KeyRelease {
                        keyval: keyval.into_glib(),
                    },
                ));
            });
        }
        drawing_area.add_controller(key_controller);

        {
            let draw_state = draw_state.clone();
            let view = view.clone();
            drawing_area.set_draw_func(move |_, cr, width, height| {
                let st = draw_state.borrow();
                if !st.connected {
                    return;
                }

                if st.frame_received {
                    let w = st.width as i32;
                    let h = st.height as i32;

                    cr.set_source_rgb(0.04, 0.05, 0.09);
                    cr.paint().unwrap();

                    let (scale, offset_x, offset_y) = placement(
                        (width as f64, height as f64),
                        (w as f64, h as f64),
                        view.get(),
                    );

                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
                    cr.rectangle(
                        offset_x + 8.0,
                        offset_y + 8.0,
                        w as f64 * scale,
                        h as f64 * scale,
                    );
                    cr.fill().unwrap();

                    cr.set_source_rgba(0.27, 0.95, 1.0, 0.35);
                    cr.set_line_width(2.0);
                    cr.rectangle(
                        offset_x - 1.0,
                        offset_y - 1.0,
                        w as f64 * scale + 2.0,
                        h as f64 * scale + 2.0,
                    );
                    cr.stroke().unwrap();

                    cr.save().unwrap();
                    cr.rectangle(offset_x, offset_y, w as f64 * scale, h as f64 * scale);
                    cr.clip();

                    cr.translate(offset_x, offset_y);
                    cr.scale(scale, scale);

                    if let Ok(mut surface) =
                        gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h)
                    {
                        {
                            if let Ok(mut data) = surface.data() {
                                if st.bgra.len() >= (w * h * 4) as usize {
                                    data.copy_from_slice(&st.bgra[..(w * h * 4) as usize]);
                                }
                            }
                        }
                        let _ = cr.set_source_surface(&surface, 0.0, 0.0);
                        let _ = cr.paint();
                    }
                    cr.restore().unwrap();
                    return;
                }

                cr.set_source_rgb(0.04, 0.05, 0.09);
                cr.paint().unwrap();

                cr.set_source_rgb(0.6, 0.65, 0.7);
                cr.select_font_face(
                    "sans-serif",
                    gtk::cairo::FontSlant::Normal,
                    gtk::cairo::FontWeight::Bold,
                );
                cr.set_font_size(18.0);
                let text = crate::i18n::tr("rdesk.awaiting_stream");
                let text = text.as_str();
                if let Ok(extents) = cr.text_extents(text) {
                    let text_w = extents.width();
                    let text_h = extents.height();
                    cr.move_to(
                        (width as f64 - text_w) / 2.0,
                        (height as f64 + text_h) / 2.0,
                    );
                    cr.show_text(text).unwrap();
                }
            });
        }

        {
            let mirror = mirror.clone();
            let orig_sw = original_size_switch.clone();
            let force_sw = force_select_switch.clone();
            let quality_dd = quality_combo.clone();
            let sender = sender.clone();
            connect_btn.connect_clicked(move |_| {
                let m = mirror.get();
                if m.local {
                    return;
                }
                let _ = sender.output(RemoteDesktopOutput::Connect {
                    connect: !m.connected,
                    original_size: orig_sw.is_active(),
                    bitrate_bps: Some(selected_bitrate(&quality_dd)),
                    force_select: force_sw.is_active(),
                });
            });
        }
        {
            let mirror = mirror.clone();
            let orig_sw = original_size_switch.clone();
            let force_sw = force_select_switch.clone();
            let quality_dd = quality_combo.clone();
            let sender = sender.clone();
            dash_connect_btn.connect_clicked(move |_| {
                let m = mirror.get();
                if m.local || m.connected {
                    return;
                }
                let _ = sender.output(RemoteDesktopOutput::Connect {
                    connect: true,
                    original_size: orig_sw.is_active(),
                    bitrate_bps: Some(selected_bitrate(&quality_dd)),
                    force_select: force_sw.is_active(),
                });
            });
        }

        let sender_control = sender.clone();
        let control_handler = control_switch.connect_active_notify(move |sw| {
            let _ = sender_control.output(RemoteDesktopOutput::SetControl(sw.is_active()));
        });
        let sender_orig = sender.clone();
        let original_size_handler = original_size_switch.connect_active_notify(move |sw| {
            sender_orig.input(RemoteDesktopInput::SetOriginalSize(sw.is_active()));
        });
        let sender_force = sender.clone();
        let force_select_handler = force_select_switch.connect_active_notify(move |sw| {
            sender_force.input(RemoteDesktopInput::SetForceSelect(sw.is_active()));
        });
        let sender_quality = sender.clone();
        quality_combo.connect_selected_notify(move |dd| {
            sender_quality.input(RemoteDesktopInput::SetBitrate(selected_bitrate(dd)));
        });

        let model = RemoteDesktopModel {
            show_toolbar: init.show_toolbar,
            resource_id: String::new(),
            local: false,
            connected: false,
            controlling: true,
            original_size: false,
            bitrate_bps: BITRATES[1].0,
            force_select: false,
            remote_w: 1280,
            remote_h: 720,
            error: None,
            stats_text: crate::i18n::tr("rdesk.awaiting_frames"),
            window_bytes: 0,
            window_frames: 0,
            frame_generation: 0,
            draw_state,
            mirror,
            geom_mirror,
            stats_gate: Rc::new(Cell::new(false)),
            stats_timer_armed: Rc::new(Cell::new(false)),
        };

        let widgets = RemoteDesktopWidgets {
            header,
            status_lbl,
            connect_btn,
            control_switch,
            control_handler,
            original_size_switch,
            original_size_handler,
            force_select_switch,
            force_select_handler,
            stack,
            drawing_area,
            rendered_frame_generation: 0,
            rendered_presentation: (false, false),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            RemoteDesktopInput::SetSession { resource_id, local } => {
                self.resource_id = resource_id;
                self.local = local;
                self.error = None;
                if self.connected {
                    self.connected = false;
                    self.force_select = false;
                }
                self.stats_gate.set(false);
                self.window_bytes = 0;
                self.window_frames = 0;
                self.stats_text = crate::i18n::tr("rdesk.awaiting_frames");
                self.draw_state.borrow_mut().frame_received = false;
            }
            RemoteDesktopInput::ConnectionChanged {
                connected,
                remote_w,
                remote_h,
            } => {
                if connected {
                    self.connected = true;
                    self.error = None;
                    if remote_w > 0 && remote_h > 0 {
                        self.remote_w = remote_w;
                        self.remote_h = remote_h;
                    }
                    self.window_bytes = 0;
                    self.window_frames = 0;
                    self.stats_text = crate::i18n::tr("rdesk.awaiting_frames");
                    self.stats_gate.set(true);
                    self.arm_stats_timer_if_needed(&sender);
                } else {
                    self.connected = false;
                    self.force_select = false;
                    self.stats_gate.set(false);
                    self.draw_state.borrow_mut().frame_received = false;
                }
            }
            RemoteDesktopInput::ConnectionError(msg) => {
                self.connected = false;
                self.force_select = false;
                self.stats_gate.set(false);
                self.error = Some(msg);
                self.draw_state.borrow_mut().frame_received = false;
            }
            RemoteDesktopInput::ControlChanged(controlling) => {
                self.controlling = controlling;
            }
            RemoteDesktopInput::Frame {
                width,
                height,
                bgra,
                compressed_len,
            } => {
                {
                    let mut ds = self.draw_state.borrow_mut();
                    ds.width = width;
                    ds.height = height;
                    ds.bgra = bgra;
                    ds.frame_received = true;
                }
                self.frame_generation += 1;
                self.window_bytes += compressed_len;
                self.window_frames += 1;
            }
            RemoteDesktopInput::StatsTick => {
                if self.connected {
                    let bytes = self.window_bytes;
                    let frames = self.window_frames;
                    self.window_bytes = 0;
                    self.window_frames = 0;

                    let speed_kb = bytes as f64 / 1024.0;
                    let speed_mb = (bytes as f64 * 8.0) / (1000.0 * 1000.0);
                    let speed_text = if speed_kb > 1024.0 {
                        format!("{:.2} Mb/s", speed_mb)
                    } else {
                        format!("{:.0} KB/s", speed_kb)
                    };
                    self.stats_text = format!("({} • {} FPS)", speed_text, frames);
                }
            }
            RemoteDesktopInput::SetOriginalSize(v) => {
                self.original_size = v;
                if self.connected {
                    let _ = sender.output(RemoteDesktopOutput::Connect {
                        connect: true,
                        original_size: v,
                        bitrate_bps: Some(self.bitrate_bps),
                        force_select: self.force_select,
                    });
                }
            }
            RemoteDesktopInput::ScreenSize { width, height } => {
                if width > 0 && height > 0 && (self.remote_w, self.remote_h) != (width, height) {
                    self.remote_w = width;
                    self.remote_h = height;
                    self.sync_mirrors();
                }
            }
            RemoteDesktopInput::SetBitrate(v) => {
                self.bitrate_bps = v;
                if self.connected {
                    let _ = sender.output(RemoteDesktopOutput::Connect {
                        connect: true,
                        original_size: self.original_size,
                        bitrate_bps: Some(v),
                        force_select: self.force_select,
                    });
                }
            }
            RemoteDesktopInput::SetForceSelect(v) => {
                self.force_select = v;
                if self.connected {
                    let _ = sender.output(RemoteDesktopOutput::Connect {
                        connect: true,
                        original_size: self.original_size,
                        bitrate_bps: Some(self.bitrate_bps),
                        force_select: v,
                    });
                }
            }
        }
        self.sync_mirrors();
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.header.set_visible(self.show_toolbar);

        widgets
            .control_switch
            .block_signal(&widgets.control_handler);
        widgets.control_switch.set_active(self.controlling);
        widgets
            .control_switch
            .unblock_signal(&widgets.control_handler);

        widgets
            .original_size_switch
            .block_signal(&widgets.original_size_handler);
        widgets.original_size_switch.set_active(self.original_size);
        widgets
            .original_size_switch
            .unblock_signal(&widgets.original_size_handler);

        widgets
            .force_select_switch
            .block_signal(&widgets.force_select_handler);
        widgets.force_select_switch.set_active(self.force_select);
        widgets
            .force_select_switch
            .unblock_signal(&widgets.force_select_handler);

        let (status, active) = if let Some(err) = &self.error {
            (
                crate::i18n::trf("rdesk.error_fmt", &[("error", err)]),
                false,
            )
        } else if self.local {
            (crate::i18n::tr("rdesk.local_host"), true)
        } else if self.connected {
            (
                crate::i18n::trf(
                    "rdesk.connected_fmt",
                    &[
                        ("w", &self.remote_w.to_string()),
                        ("h", &self.remote_h.to_string()),
                        ("stats", &self.stats_text),
                    ],
                ),
                true,
            )
        } else {
            (crate::i18n::tr("rdesk.disconnected"), false)
        };
        widgets.status_lbl.set_label(&status);
        if active {
            widgets.status_lbl.remove_css_class("rdesk-status-inactive");
            widgets.status_lbl.add_css_class("rdesk-status-active");
        } else {
            widgets.status_lbl.remove_css_class("rdesk-status-active");
            widgets.status_lbl.add_css_class("rdesk-status-inactive");
        }

        if self.connected {
            widgets
                .connect_btn
                .set_label(&crate::i18n::tr("rdesk.disconnect"));
            widgets.connect_btn.remove_css_class("suggested-action");
            widgets.connect_btn.add_css_class("destructive-action");
        } else {
            widgets
                .connect_btn
                .set_label(&crate::i18n::tr("rdesk.connect_session"));
            widgets.connect_btn.remove_css_class("destructive-action");
            widgets.connect_btn.add_css_class("suggested-action");
        }
        widgets.connect_btn.set_visible(!self.local);

        let session = self.connected || self.local;
        widgets
            .stack
            .set_visible_child_name(if session { "session" } else { "dashboard" });

        let presentation = (session, self.draw_state.borrow().frame_received);
        if widgets.rendered_frame_generation != self.frame_generation
            || widgets.rendered_presentation != presentation
        {
            widgets.rendered_frame_generation = self.frame_generation;
            widgets.rendered_presentation = presentation;
            widgets.drawing_area.queue_draw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{get_translated_coords, remap_button};

    #[test]
    fn smaller_frame_is_enlarged_to_fill() {
        let da = (800.0, 600.0);
        let frame = (400.0, 300.0);
        let screen = (800.0, 600.0);
        assert_eq!(
            get_translated_coords(da, (0.0, 0.0), frame, screen),
            Some((0, 0))
        );
        assert_eq!(
            get_translated_coords(da, (400.0, 300.0), frame, screen),
            Some((400, 300))
        );
    }

    #[test]
    fn letterbox_margins_are_dropped() {
        let da = (800.0, 600.0);
        let frame = (400.0, 400.0);
        let screen = (2000.0, 2000.0);
        assert_eq!(
            get_translated_coords(da, (100.0, 0.0), frame, screen),
            Some((0, 0))
        );
        assert_eq!(
            get_translated_coords(da, (400.0, 300.0), frame, screen),
            Some((1000, 1000))
        );
        assert_eq!(
            get_translated_coords(da, (50.0, 300.0), frame, screen),
            None,
            "the margin belongs to no pixel of the host screen"
        );
    }

    #[test]
    fn larger_frame_scales_down_only() {
        let da = (400.0, 300.0);
        let frame = (800.0, 600.0);
        let screen = (1600.0, 1200.0);
        assert_eq!(
            get_translated_coords(da, (200.0, 150.0), frame, screen),
            Some((800, 600))
        );
        assert_eq!(
            get_translated_coords(da, (0.0, 0.0), frame, screen),
            Some((0, 0))
        );
    }

    #[test]
    fn zero_sized_frame_yields_none() {
        assert_eq!(
            get_translated_coords((800.0, 600.0), (10.0, 10.0), (0.0, 0.0), (800.0, 600.0)),
            None
        );
    }

    #[test]
    fn gtk_buttons_remap_to_protocol() {
        assert_eq!(remap_button(1), 1);
        assert_eq!(remap_button(3), 2);
        assert_eq!(remap_button(2), 3);
        assert_eq!(remap_button(9), 9);
    }
}
