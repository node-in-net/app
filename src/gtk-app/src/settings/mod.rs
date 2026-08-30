use adw::prelude::*;
use app_headless::ApiCmd;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) mod page_about;
pub(crate) mod page_connection;
pub(crate) mod page_general;
pub(crate) mod page_network;
pub(crate) mod page_sharing;

pub(crate) use page_sharing::service_config;

pub(crate) fn caps_label(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("caps");
    l.set_halign(gtk::Align::Center);
    l
}

pub(crate) fn apply_stored_limits(config: &client_config::AppConfig) {
    client_core::limits::set_max_peers(config.get::<u32>("ui.max_peers").unwrap_or(0));
    client_core::limits::set_bandwidth_limit_kbps(
        config.get::<u32>("ui.bandwidth_limit").unwrap_or(0),
    );
    page_connection::apply_stored(config);
}

pub(crate) fn open_settings_dialog(
    parent: Option<gtk::Window>,
    cmd: &UnboundedSender<ApiCmd>,
    config: &client_config::AppConfig,
) {
    let win = adw::Window::builder()
        .title(crate::i18n::tr("settings.title"))
        .modal(true)
        .default_width(760)
        .default_height(560)
        .build();

    let sidebar = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    sidebar.add_css_class("navigation-sidebar");

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .hexpand(true)
        .vexpand(true)
        .build();

    type PageBuilder = Box<dyn Fn(&gtk::Box)>;
    let builders: Rc<RefCell<Vec<Option<PageBuilder>>>> = Rc::new(RefCell::new(Vec::new()));
    let pages: Rc<RefCell<Vec<gtk::Box>>> = Rc::new(RefCell::new(Vec::new()));

    let categories: Vec<(&str, String)> = vec![
        ("sharing", crate::i18n::tr("settings.cat_sharing")),
        ("network", crate::i18n::tr("settings.cat_network")),
        ("connection", crate::i18n::tr("settings.cat_connection")),
        ("general", crate::i18n::tr("settings.cat_general")),
        ("about", crate::i18n::tr("settings.cat_about")),
    ];

    let mut first_row = None;
    for (i, (key, title)) in categories.iter().enumerate() {
        let row = gtk::ListBoxRow::new();
        let label = gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .margin_start(16)
            .margin_end(16)
            .margin_top(10)
            .margin_bottom(10)
            .build();
        row.set_child(Some(&label));
        sidebar.append(&row);
        if i == 0 {
            first_row = Some(row);
        }

        let page_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();

        let builder: Option<PageBuilder> = match *key {
            "sharing" => {
                let cmd = cmd.clone();
                let config = config.clone();
                Some(Box::new(move |b: &gtk::Box| {
                    b.append(&page_sharing::service_config(&cmd, &config).widget);
                }))
            }
            "network" => {
                let cmd = cmd.clone();
                let config = config.clone();
                Some(Box::new(move |b: &gtk::Box| {
                    b.append(&page_network::relay_region(&cmd, &config));
                    b.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
                    b.append(&page_network::network_limits(&config));
                }))
            }
            "connection" => {
                let config = config.clone();
                Some(Box::new(move |b: &gtk::Box| {
                    b.append(&page_connection::build(&config));
                }))
            }
            "general" => {
                let cmd = cmd.clone();
                let config = config.clone();
                Some(Box::new(move |b: &gtk::Box| {
                    b.append(&page_general::theme(&cmd, &config));
                    b.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
                    b.append(&page_general::language(&config));
                }))
            }
            "about" => {
                let win = win.clone();
                Some(Box::new(move |b: &gtk::Box| {
                    b.append(&page_about::about(win.clone().upcast_ref::<gtk::Window>()));
                }))
            }
            _ => None,
        };
        builders.borrow_mut().push(builder);
        pages.borrow_mut().push(page_box.clone());

        stack.add_named(
            &gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .child(&page_box)
                .build(),
            Some(&format!("page_{i}")),
        );
    }

    {
        let stack = stack.clone();
        let builders = builders.clone();
        let pages = pages.clone();
        sidebar.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let index = row.index() as usize;
            let builder = builders.borrow_mut().get_mut(index).and_then(Option::take);
            let page_box = pages.borrow().get(index).cloned();
            if let (Some(build), Some(page_box)) = (builder, page_box) {
                build(&page_box);
            }
            stack.set_visible_child_name(&format!("page_{index}"));
        });
    }
    if let Some(row) = first_row {
        sidebar.select_row(Some(&row));
    }

    let split = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let side = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .width_request(190)
        .child(&sidebar)
        .build();
    split.append(&side);
    split.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    split.append(&stack);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&split));
    win.set_content(Some(&toolbar));

    let keys = gtk::EventControllerKey::new();
    let esc = win.clone();
    keys.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            esc.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    win.add_controller(keys);

    if let Some(p) = parent {
        win.set_transient_for(Some(&p));
    }
    win.present();
}
