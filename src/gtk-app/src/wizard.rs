use adw::prelude::*;
use app_core::session::{SessionState, Stage};
use app_core::workspace::ServiceKind;
use app_headless::ApiCmd;
use relm4::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::icons;

pub struct WizardInit {
    pub cmd: UnboundedSender<ApiCmd>,
    pub config: client_config::AppConfig,
}

#[derive(Debug)]
pub enum WizardInput {
    Render(Box<SessionState>),
    SetField { field: String, value: String },
    SetService { kind: ServiceKind, on: bool },
    Submit,
}

pub struct WizardModel {
    cmd: UnboundedSender<ApiCmd>,
    carousel: adw::Carousel,
    pages: [gtk::Widget; 4],
    name_entry: gtk::Entry,
    detected_label: gtk::Label,
    login_entry: gtk::Entry,
    password_entry: gtk::PasswordEntry,
    guest_check: gtk::CheckButton,
    error_label: gtk::Label,
    allset_title: gtk::Label,
    allset_sub: gtk::Label,
    service_switches: Vec<(ServiceKind, gtk::Switch)>,
    last_stage: String,
}

fn report_field(entry: &gtk::Entry, field: &str, cmd: &UnboundedSender<ApiCmd>) {
    let cmd = cmd.clone();
    let field = field.to_string();
    entry.connect_changed(move |e| {
        let _ = cmd.send(ApiCmd::ReportField {
            field: field.clone(),
            value: e.text().to_string(),
        });
    });
}

impl WizardModel {
    pub fn window_title(state: &SessionState) -> String {
        match state.stage {
            Stage::NamingDevice { .. } => crate::i18n::tr("wizard.title_setup"),
            Stage::SigningIn => crate::i18n::tr("wizard.title_signin"),
            _ => "Node.In.Net".to_string(),
        }
    }
}

fn value_prop(icon: &str, caption: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 5);
    b.set_width_request(96);
    b.append(&icons::image(icon, 40));
    let l = gtk::Label::new(Some(caption));
    l.add_css_class("vp-label");
    b.append(&l);
    b
}

fn value_props(files_caption: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_halign(gtk::Align::Center);
    row.append(&value_prop("sysinfo", &crate::i18n::tr("services.sysinfo")));
    row.append(&value_prop("fileexplorer", files_caption));
    row.append(&value_prop(
        "unix-console",
        &crate::i18n::tr("services.terminal"),
    ));
    row.append(&value_prop("display", &crate::i18n::tr("services.desktop")));
    row.append(&value_prop("vpn", &crate::i18n::tr("services.network")));
    if cfg!(target_os = "windows") {
        row.append(&value_prop(
            "registry",
            &crate::i18n::tr("services.registry"),
        ));
    }
    row
}

fn reassure(text: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_halign(gtk::Align::Center);
    row.append(&icons::image("agreement", 14));
    let l = gtk::Label::new(Some(text));
    l.add_css_class("reassure");
    row.append(&l);
    row
}

fn title(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("title-1");
    l
}

fn subtitle(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("wiz-sub");
    l.set_wrap(true);
    l.set_justify(gtk::Justification::Center);
    l.set_max_width_chars(46);
    l
}

fn caps_label(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("caps");
    l.set_halign(gtk::Align::Start);
    l
}

fn page(children: &[&gtk::Widget]) -> gtk::Widget {
    let inner = gtk::Box::new(gtk::Orientation::Vertical, 14);
    inner.set_valign(gtk::Align::Center);
    inner.set_halign(gtk::Align::Center);
    inner.set_margin_top(24);
    inner.set_margin_bottom(24);
    for child in children {
        inner.append(*child);
    }
    let clamp = adw::Clamp::builder()
        .maximum_size(440)
        .child(&inner)
        .build();
    clamp.set_hexpand(true);
    clamp.upcast()
}

fn field_clamp(widget: &impl IsA<gtk::Widget>) -> adw::Clamp {
    adw::Clamp::builder()
        .maximum_size(360)
        .child(widget)
        .build()
}

impl SimpleComponent for WizardModel {
    type Init = WizardInit;
    type Input = WizardInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_margin_bottom(12);
        root
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let cmd = init.cmd;
        let config = init.config;
        let send = {
            let cmd = cmd.clone();
            std::rc::Rc::new(move |api_cmd: ApiCmd| {
                let _ = cmd.send(api_cmd);
            })
        };

        let get_started = gtk::Button::with_label(&crate::i18n::tr("wizard.get_started"));
        get_started.add_css_class("suggested-action");
        get_started.add_css_class("pill");
        get_started.set_halign(gtk::Align::Center);
        {
            let send = send.clone();
            get_started.connect_clicked(move |_| send(ApiCmd::SetupBegin));
        }
        let welcome = page(&[
            icons::image("connect", 96).upcast_ref(),
            title(&crate::i18n::tr("wizard.welcome_title")).upcast_ref(),
            subtitle(&crate::i18n::tr("wizard.welcome_sub")).upcast_ref(),
            value_props(&crate::i18n::tr("services.files")).upcast_ref(),
            get_started.upcast_ref(),
            reassure(&crate::i18n::tr("wizard.reassure_p2p")).upcast_ref(),
        ]);

        let name_entry = gtk::Entry::new();
        name_entry.set_hexpand(true);
        report_field(&name_entry, "name", &cmd);
        let name_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
        name_box.append(&caps_label(&crate::i18n::tr("wizard.device_name_caps")));
        name_box.append(&name_entry);
        let detected_label = gtk::Label::new(None);
        detected_label.add_css_class("reassure");

        let back = gtk::Button::with_label(&crate::i18n::tr("wizard.back"));
        back.add_css_class("pill");
        {
            let send = send.clone();
            back.connect_clicked(move |_| send(ApiCmd::AuthLogout));
        }
        let cont = gtk::Button::with_label(&crate::i18n::tr("wizard.continue"));
        cont.add_css_class("suggested-action");
        cont.add_css_class("pill");
        {
            let send = send.clone();
            let entry = name_entry.clone();
            cont.connect_clicked(move |_| {
                send(ApiCmd::SetupName {
                    name: entry.text().to_string(),
                })
            });
        }
        {
            let send = send.clone();
            name_entry.connect_activate(move |e| {
                send(ApiCmd::SetupName {
                    name: e.text().to_string(),
                })
            });
        }
        let nav = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        nav.set_halign(gtk::Align::Center);
        nav.append(&back);
        nav.append(&cont);

        let os = std::env::consts::OS;
        let naming = page(&[
            icons::image(icons::desktop_hero_name(os), 72).upcast_ref(),
            title(&crate::i18n::tr("wizard.name_title")).upcast_ref(),
            subtitle(&crate::i18n::tr("wizard.name_sub")).upcast_ref(),
            field_clamp(&name_box).upcast_ref(),
            detected_label.upcast_ref(),
            nav.upcast_ref(),
        ]);

        let login_entry = gtk::Entry::new();
        report_field(&login_entry, "login", &cmd);
        let password_entry = gtk::PasswordEntry::new();
        password_entry.set_show_peek_icon(true);
        let login_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
        login_box.append(&caps_label(&crate::i18n::tr("wizard.login_caps")));
        login_box.append(&login_entry);
        let password_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
        password_box.append(&caps_label(&crate::i18n::tr("wizard.password_caps")));
        password_box.append(&password_entry);

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error-label");
        error_label.set_visible(false);
        error_label.set_wrap(true);

        let sign_in = gtk::Button::new();
        let sign_in_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        sign_in_content.set_halign(gtk::Align::Center);
        sign_in_content.append(&icons::image("login", 18));
        sign_in_content.append(&gtk::Label::new(Some(&crate::i18n::tr(
            "wizard.secure_sign_in",
        ))));
        sign_in.set_child(Some(&sign_in_content));
        sign_in.add_css_class("suggested-action");
        sign_in.add_css_class("pill");
        let guest_check = gtk::CheckButton::builder()
            .label(&crate::i18n::tr("wizard.guest_session"))
            .halign(gtk::Align::Center)
            .build();
        {
            let send = send.clone();
            let login = login_entry.clone();
            let password = password_entry.clone();
            let guest = guest_check.clone();
            sign_in.connect_clicked(move |_| {
                send(ApiCmd::AuthLogin {
                    login: login.text().to_string(),
                    password: password.text().to_string(),
                    guest: guest.is_active(),
                })
            });
        }
        let guest = gtk::Button::with_label(&crate::i18n::tr("wizard.guest"));
        guest.add_css_class("flat");
        guest.add_css_class("link-accent");
        guest.set_halign(gtk::Align::Center);
        {
            let send = send.clone();
            guest.connect_clicked(move |_| send(ApiCmd::AuthGuest));
        }

        let signin = page(&[
            icons::image("login", 72).upcast_ref(),
            title(&crate::i18n::tr("wizard.signin_title")).upcast_ref(),
            subtitle(&crate::i18n::tr("wizard.signin_sub")).upcast_ref(),
            field_clamp(&login_box).upcast_ref(),
            field_clamp(&password_box).upcast_ref(),
            error_label.upcast_ref(),
            guest_check.upcast_ref(),
            field_clamp(&sign_in).upcast_ref(),
            guest.upcast_ref(),
            reassure(&crate::i18n::tr("wizard.reassure_e2e")).upcast_ref(),
        ]);

        let allset_title = title(&crate::i18n::tr("wizard.all_set"));
        let allset_sub = subtitle("");
        let open_ws = gtk::Button::with_label(&crate::i18n::tr("wizard.open_workspace"));
        open_ws.add_css_class("suggested-action");
        open_ws.add_css_class("pill");
        open_ws.set_halign(gtk::Align::Center);
        {
            let send = send.clone();
            open_ws.connect_clicked(move |_| send(ApiCmd::EnterWorkspace));
        }
        let toggles = crate::settings::service_config(&cmd, &config);
        let allset = page(&[
            icons::image("done", 96).upcast_ref(),
            allset_title.upcast_ref(),
            allset_sub.upcast_ref(),
            toggles.widget.upcast_ref(),
            open_ws.upcast_ref(),
        ]);

        let carousel = adw::Carousel::new();
        carousel.set_allow_mouse_drag(false);
        carousel.set_allow_scroll_wheel(false);
        carousel.set_allow_long_swipes(false);
        carousel.set_vexpand(true);
        carousel.set_spacing(64);
        carousel.set_overflow(gtk::Overflow::Hidden);
        for p in [&welcome, &naming, &signin, &allset] {
            carousel.append(p);
        }
        let dots = adw::CarouselIndicatorDots::new();
        dots.set_carousel(Some(&carousel));

        root.append(&carousel);
        root.append(&dots);

        let model = WizardModel {
            cmd,
            carousel,
            pages: [welcome, naming, signin, allset],
            name_entry,
            detected_label,
            login_entry,
            password_entry,
            guest_check,
            error_label,
            allset_title,
            allset_sub,
            service_switches: toggles.switches,
            last_stage: String::new(),
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WizardInput::Render(state) => self.render(&state),
            WizardInput::SetField { field, value } => match field.as_str() {
                "name" => self.name_entry.set_text(&value),
                "login" => self.login_entry.set_text(&value),
                "password" => self.password_entry.set_text(&value),
                _ => {}
            },
            WizardInput::SetService { kind, on } => {
                if let Some((_, sw)) = self.service_switches.iter().find(|(k, _)| *k == kind) {
                    sw.set_active(on);
                }
            }
            WizardInput::Submit => {
                let cmd = match self.last_stage.as_str() {
                    "naming_device" => ApiCmd::SetupName {
                        name: self.name_entry.text().to_string(),
                    },
                    "signing_in" => ApiCmd::AuthLogin {
                        login: self.login_entry.text().to_string(),
                        password: self.password_entry.text().to_string(),
                        guest: self.guest_check.is_active(),
                    },
                    "all_set" => ApiCmd::EnterWorkspace,
                    _ => ApiCmd::SetupBegin,
                };
                let _ = self.cmd.send(cmd);
            }
        }
    }
}

impl WizardModel {
    fn render(&mut self, state: &SessionState) {
        let (index, stage_key) = match &state.stage {
            Stage::Welcome => (0, "welcome"),
            Stage::NamingDevice { .. } => (1, "naming_device"),
            Stage::SigningIn => (2, "signing_in"),
            Stage::AllSet => (3, "all_set"),
            Stage::Ready => (3, "ready"),
        };

        let changed = self.last_stage != stage_key;
        if changed {
            self.last_stage = stage_key.to_string();
            self.carousel.scroll_to(&self.pages[index], true);
            let _ = self.cmd.send(ApiCmd::ReportVisiblePage {
                name: stage_key.to_string(),
            });

            if let Stage::NamingDevice { suggested_name } = &state.stage {
                self.name_entry.set_text(suggested_name);
                self.detected_label.set_text(&crate::i18n::trf(
                    "wizard.detected",
                    &[("name", icons::os_pretty(std::env::consts::OS))],
                ));
                self.name_entry.grab_focus();
            }
            if matches!(state.stage, Stage::SigningIn) {
                self.password_entry.set_text("");
                self.login_entry.grab_focus();
            }
            if matches!(state.stage, Stage::AllSet) {
                let who = state
                    .account_login
                    .as_deref()
                    .map(|l| crate::i18n::trf("wizard.all_set_named", &[("login", l)]))
                    .unwrap_or_else(|| crate::i18n::tr("wizard.all_set"));
                self.allset_title.set_text(&who);
                let this_device = crate::i18n::tr("wizard.this_device");
                let device = state.device_name.as_deref().unwrap_or(&this_device);
                self.allset_sub.set_text(&crate::i18n::trf(
                    "wizard.connected_to_network",
                    &[("device", device)],
                ));
            }
        }

        match &state.last_error {
            Some(err) => {
                self.error_label.set_text(err);
                self.error_label.set_visible(true);
            }
            None => self.error_label.set_visible(false),
        }
    }
}
