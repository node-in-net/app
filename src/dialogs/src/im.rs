use gtk::prelude::*;

/// Gives a custom key widget (a terminal, a remote screen) the soft keyboard on touch: a tap
/// focuses it and shows the keyboard, typed text arrives through `on_commit`, and Backspace,
/// Enter and the like still arrive as key presses through `keys`. Keep the returned context
/// alive as long as the widget. Nothing is attached on the desktop, so no input method there
/// sees the keys before the widget does.
pub fn attach_soft_keyboard(
    widget: &impl IsA<gtk::Widget>,
    keys: &gtk::EventControllerKey,
    purpose: gtk::InputPurpose,
    on_commit: impl Fn(&str) + 'static,
) -> Option<gtk::IMMulticontext> {
    if !crate::touch() {
        return None;
    }
    let widget = widget.as_ref();
    widget.set_focusable(true);
    let im = gtk::IMMulticontext::new();
    im.set_client_widget(Some(widget));
    im.set_input_purpose(purpose);
    im.set_input_hints(gtk::InputHints::NO_SPELLCHECK | gtk::InputHints::NO_EMOJI);
    im.connect_commit(move |_, text| on_commit(text));
    keys.set_im_context(Some(&im));

    let focus = gtk::EventControllerFocus::new();
    {
        let im = im.clone();
        focus.connect_enter(move |_| im.focus_in());
    }
    {
        let im = im.clone();
        focus.connect_leave(move |_| im.focus_out());
    }
    widget.add_controller(focus);

    // Every tap shows the keyboard again, even after the user hid it.
    let tap = gtk::GestureClick::new();
    {
        let im = im.clone();
        tap.connect_released(move |g, _, _, _| {
            if let Some(w) = g.widget() {
                w.grab_focus();
            }
            im.focus_in();
        });
    }
    widget.add_controller(tap);
    Some(im)
}
