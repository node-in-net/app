use gtk::{gdk, glib::translate::ToGlibPtr, prelude::*};
use jni::{
    errors::{Error, Result as JniResult},
    objects::{JObject, JValue},
    JNIEnv,
};

extern "C" {
    fn gdk_android_display_get_env(display: *mut gdk::ffi::GdkDisplay) -> *mut jni::sys::JNIEnv;
    fn gdk_android_toplevel_get_activity(toplevel: *mut gdk::ffi::GdkSurface) -> jni::sys::jobject;
}

/// Local files are real paths in the core, so they need All-files access, as in the Kotlin client.
pub fn ensure_all_files_access(window: &gtk::Window) {
    let asked = with_activity(window, |env, activity| {
        let granted = env
            .call_static_method(
                "android/os/Environment",
                "isExternalStorageManager",
                "()Z",
                &[],
            )?
            .z()?;
        if granted {
            return Ok(());
        }
        let action = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
        let package = env.new_string(format!("package:{}", crate::APP_ID))?;
        let uri = env
            .call_static_method(
                "android/net/Uri",
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[JValue::Object(&package)],
            )?
            .l()?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        env.call_method(
            activity,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    });
    if let Err(e) = asked {
        gtk::glib::g_warning!("nodeinnet", "cannot ask for All-files access: {e}");
    }
}

pub fn open_browser(window: &gtk::Window, proxy_port: u16) -> bool {
    let opened = with_activity(window, |env, activity| {
        let class = env.new_string(format!("{}.BrowserActivity", crate::APP_ID))?;
        let package = env.new_string(crate::APP_ID)?;
        let intent = env.new_object("android/content/Intent", "()V", &[])?;
        env.call_method(
            &intent,
            "setClassName",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&package), JValue::Object(&class)],
        )?;
        let key = env.new_string("proxy_port")?;
        env.call_method(
            &intent,
            "putExtra",
            "(Ljava/lang/String;I)Landroid/content/Intent;",
            &[JValue::Object(&key), JValue::Int(proxy_port.into())],
        )?;
        env.call_method(
            activity,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    });
    opened
        .map_err(|e| gtk::glib::g_warning!("nodeinnet", "cannot open the browser: {e}"))
        .is_ok()
}

/// Back on the first screen: the task goes behind and the node keeps running, as in any Android app.
pub fn put_away(window: &gtk::Window) {
    let moved = with_activity(window, |env, activity| {
        env.call_method(activity, "moveTaskToBack", "(Z)Z", &[JValue::Bool(1)])?
            .z()
    });
    if let Err(e) = moved {
        gtk::glib::g_warning!("nodeinnet", "cannot move the app to the background: {e}");
    }
}

fn with_activity<T>(
    window: &gtk::Window,
    f: impl FnOnce(&mut JNIEnv, &JObject) -> JniResult<T>,
) -> Result<T, String> {
    let surface = window.surface().ok_or("the window has no surface")?;
    let display = WidgetExt::display(window);
    // SAFETY: GTK keeps its thread attached to the JVM and returns that thread's env.
    let mut env =
        unsafe { JNIEnv::from_raw(gdk_android_display_get_env(display.to_glib_none().0)) }
            .map_err(|e| e.to_string())?;
    env.with_local_frame(8, |env| -> JniResult<T> {
        // SAFETY: a new local reference, released with this frame.
        let activity = unsafe {
            JObject::from_raw(gdk_android_toplevel_get_activity(surface.to_glib_none().0))
        };
        if activity.is_null() {
            return Err(Error::NullPtr("activity"));
        }
        let result = f(env, &activity);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        result
    })
    .map_err(|e| e.to_string())
}
