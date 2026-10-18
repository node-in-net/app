use adw::prelude::*;

/// Right click, and on touch a long press: both call `f` with the point. `f` returns whether
/// it handled the press; if not, the press goes on to the widgets underneath.
pub fn on_context(
    widget: &impl IsA<gtk::Widget>,
    f: impl Fn(&gtk::Widget, f64, f64) -> bool + 'static,
) {
    let f = std::rc::Rc::new(f);
    let click = gtk::GestureClick::builder()
        .button(gtk::gdk::BUTTON_SECONDARY)
        .build();
    {
        let f = f.clone();
        click.connect_pressed(move |g, _, x, y| {
            if let Some(w) = g.widget() {
                if f(&w, x, y) {
                    g.set_state(gtk::EventSequenceState::Claimed);
                }
            }
        });
    }
    widget.add_controller(click);

    if !crate::touch() {
        return;
    }
    let press = gtk::GestureLongPress::builder().touch_only(true).build();
    press.connect_pressed(move |g, x, y| {
        if let Some(w) = g.widget() {
            if f(&w, x, y) {
                g.set_state(gtk::EventSequenceState::Claimed);
            }
        }
    });
    widget.add_controller(press);
}

/// On touch, opens a context-menu popover's content in a bottom sheet instead and returns true:
/// popups do not survive an Android activity pause, and a sheet fits a finger. On the desktop it
/// does nothing and the caller pops the popover up as before.
pub fn sheet_instead(popover: &gtk::Popover, widget: &gtk::Widget) -> bool {
    if !crate::touch() {
        return false;
    }
    let Some(content) = popover.child() else {
        return true;
    };
    popover.set_child(gtk::Widget::NONE);
    content.add_css_class("touch-menu");
    let sheet = adw::Dialog::builder()
        .presentation_mode(adw::DialogPresentationMode::BottomSheet)
        .child(&content)
        .build();
    close_on_click(&content, &sheet);
    sheet.present(Some(widget));
    true
}

fn close_on_click(widget: &gtk::Widget, sheet: &adw::Dialog) {
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        let sheet = sheet.downgrade();
        button.connect_clicked(move |_| {
            if let Some(sheet) = sheet.upgrade() {
                sheet.close();
            }
        });
        return;
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        close_on_click(&c, sheet);
        child = c.next_sibling();
    }
}
