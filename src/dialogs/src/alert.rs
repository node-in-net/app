#![allow(deprecated)]
use adw::prelude::*;

/// A message dialog: `adw::MessageDialog` on the desktop as before, an in-window
/// `adw::AlertDialog` on touch.
pub enum Alert {
    Window(adw::MessageDialog),
    Inline(adw::AlertDialog, Option<gtk::Window>),
}

impl Alert {
    pub fn new(parent: Option<&gtk::Window>, heading: Option<&str>, body: Option<&str>) -> Self {
        if crate::touch() {
            let dialog = adw::AlertDialog::new(heading, body);
            Self::Inline(dialog, parent.cloned().or_else(active_window))
        } else {
            Self::Window(adw::MessageDialog::new(parent, heading, body))
        }
    }

    /// An in-window dialog always blocks its window.
    pub fn set_modal(&self, modal: bool) {
        if let Self::Window(d) = self {
            d.set_modal(modal);
        }
    }

    pub fn set_extra_child(&self, child: Option<&impl IsA<gtk::Widget>>) {
        match self {
            Self::Window(d) => d.set_extra_child(child),
            Self::Inline(d, _) => d.set_extra_child(child),
        }
    }

    pub fn add_response(&self, id: &str, label: &str) {
        match self {
            Self::Window(d) => d.add_response(id, label),
            Self::Inline(d, _) => d.add_response(id, label),
        }
    }

    pub fn set_default_response(&self, id: Option<&str>) {
        match self {
            Self::Window(d) => d.set_default_response(id),
            Self::Inline(d, _) => d.set_default_response(id),
        }
    }

    pub fn set_close_response(&self, id: &str) {
        match self {
            Self::Window(d) => d.set_close_response(id),
            Self::Inline(d, _) => d.set_close_response(id),
        }
    }

    pub fn set_response_appearance(&self, id: &str, appearance: adw::ResponseAppearance) {
        match self {
            Self::Window(d) => d.set_response_appearance(id, appearance),
            Self::Inline(d, _) => d.set_response_appearance(id, appearance),
        }
    }

    pub fn set_response_enabled(&self, id: &str, enabled: bool) {
        match self {
            Self::Window(d) => d.set_response_enabled(id, enabled),
            Self::Inline(d, _) => d.set_response_enabled(id, enabled),
        }
    }

    /// Both dialogs close themselves after a response.
    pub fn connect_response(&self, f: impl Fn(&str) + 'static) {
        match self {
            Self::Window(d) => {
                d.connect_response(None, move |_, r| f(r));
            }
            Self::Inline(d, _) => {
                d.connect_response(None, move |_, r| f(r));
            }
        }
    }

    pub fn present(&self) {
        match self {
            Self::Window(d) => d.present(),
            Self::Inline(d, parent) => d.present(parent.as_ref()),
        }
    }
}

pub(crate) fn active_window() -> Option<gtk::Window> {
    gtk::gio::Application::default()
        .and_downcast::<gtk::Application>()
        .and_then(|app| app.active_window())
}
