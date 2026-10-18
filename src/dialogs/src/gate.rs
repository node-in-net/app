use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Runs `tick` every `interval`, but only while `widget` is on screen.
pub fn while_mapped(
    widget: &impl IsA<gtk::Widget>,
    interval: std::time::Duration,
    tick: impl FnMut() + 'static,
) {
    let tick = Rc::new(RefCell::new(tick));
    let source: Rc<RefCell<Option<gtk::glib::SourceId>>> = Rc::default();
    {
        let source = source.clone();
        widget.connect_map(move |_| {
            let tick = tick.clone();
            let id = gtk::glib::timeout_add_local(interval, move || {
                (tick.borrow_mut())();
                gtk::glib::ControlFlow::Continue
            });
            if let Some(old) = source.replace(Some(id)) {
                old.remove();
            }
        });
    }
    widget.connect_unmap(move |_| {
        if let Some(id) = source.take() {
            id.remove();
        }
    });
}
