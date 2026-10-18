use std::sync::atomic::{AtomicBool, Ordering};

static TOUCH: AtomicBool = AtomicBool::new(false);

/// The phone build: dialogs stay inside the one window (every extra toplevel would be its own
/// Android activity), menus open as sheets, custom key widgets ask for the soft keyboard.
pub fn set_touch(on: bool) {
    TOUCH.store(on, Ordering::Relaxed);
}

pub fn touch() -> bool {
    TOUCH.load(Ordering::Relaxed)
}
