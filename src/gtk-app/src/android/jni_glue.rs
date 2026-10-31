use gtk::{gdk, glib::translate::ToGlibPtr, prelude::*};
use jni::{
    errors::{Error, Result as JniResult},
    objects::{JClass, JObject, JObjectArray, JString, JValue},
    sys::{jboolean, jint, JNI_FALSE},
    JNIEnv,
};
use std::os::fd::{FromRawFd, OwnedFd};

extern "C" {
    fn gdk_android_display_get_env(display: *mut gdk::ffi::GdkDisplay) -> *mut jni::sys::JNIEnv;
    fn gdk_android_toplevel_get_activity(toplevel: *mut gdk::ffi::GdkSurface) -> jni::sys::jobject;
}

#[link(name = "log")]
extern "C" {
    fn __android_log_write(
        prio: std::ffi::c_int,
        tag: *const std::ffi::c_char,
        text: *const std::ffi::c_char,
    ) -> std::ffi::c_int;
}

const ANDROID_LOG_WARN: std::ffi::c_int = 5;

pub fn stderr_to_logcat() {
    let mut fds = [0; 2];
    // SAFETY: pipe() fills both on success; stderr becomes the write end, which stays open.
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 || libc::dup2(fds[1], libc::STDERR_FILENO) < 0 {
            return;
        }
        libc::close(fds[1]);
    }
    // SAFETY: the read end is ours alone.
    let reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let _ = std::thread::Builder::new()
        .name("stderr".into())
        .spawn(move || {
            use std::io::BufRead;
            // Not lines(): a non-UTF-8 line would end the relay, failing every later stderr write.
            for line in std::io::BufReader::new(reader)
                .split(b'\n')
                .map_while(Result::ok)
            {
                let line = String::from_utf8_lossy(&line).replace('\0', "");
                if let Ok(text) = std::ffi::CString::new(line) {
                    // SAFETY: both are NUL-terminated and outlive the call.
                    unsafe {
                        __android_log_write(ANDROID_LOG_WARN, c"nodeinnet".as_ptr(), text.as_ptr())
                    };
                }
            }
        });
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_gtk_Core_nativeAttach(
    mut env: JNIEnv,
    _class: JClass,
    fd: jint,
    mtu: jint,
    peer: JString,
) -> jboolean {
    if fd < 0 {
        return JNI_FALSE;
    }
    // SAFETY: detachFd() gave this descriptor up, so nothing else closes it.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let Ok(peer) = env.get_string(&peer).map(String::from) else {
        return JNI_FALSE;
    };
    let mtu = u16::try_from(mtu).unwrap_or(1500);
    let (reply, answer) = tokio::sync::oneshot::channel();
    let sent = send(app_headless::ApiCmd::VpnAttach {
        fd,
        mtu,
        peer,
        reply,
    });
    // Core.java's own thread, so it may wait for a busy core.
    u8::from(sent && answer.blocking_recv().unwrap_or(false))
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_gtk_Core_nativeDetach(_env: JNIEnv, _class: JClass) {
    send(app_headless::ApiCmd::VpnDetach);
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_gtk_Core_nativeBrowserClosed(
    _env: JNIEnv,
    _class: JClass,
) {
    send(app_headless::ApiCmd::WebVpnCloseProxies);
}

#[no_mangle]
pub extern "system" fn Java_net_nodeinnet_gtk_Core_nativeCancelled(_env: JNIEnv, _class: JClass) {
    send(app_headless::ApiCmd::VpnCancelled);
}

fn send(command: app_headless::ApiCmd) -> bool {
    crate::CORE
        .get()
        .is_some_and(|core| core.send(command).is_ok())
}

pub fn start_vpn(window: &gtk::Window, request: &crate::VpnRequest) -> bool {
    let started = with_activity(window, |env, activity| {
        let vpn = app_class(env, activity, "net.nodeinnet.gtk.Vpn")?;
        let apps = env.new_object_array(
            request.apps.len() as jint,
            "java/lang/String",
            JObject::null(),
        )?;
        for (i, app) in request.apps.iter().enumerate() {
            let app = env.new_string(app)?;
            env.set_object_array_element(&apps, i as jint, &app)?;
            env.delete_local_ref(app)?;
        }
        let peer = env.new_string(request.peer)?;
        let title = env.new_string(request.title)?;
        let stop = env.new_string(request.stop)?;
        env.call_static_method(
            &vpn,
            "start",
            "(Landroid/app/Activity;[Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            &[
                JValue::Object(activity),
                JValue::Object(&apps),
                JValue::Object(&peer),
                JValue::Object(&title),
                JValue::Object(&stop),
            ],
        )?;
        Ok(())
    });
    started
        .map_err(|e| gtk::glib::g_warning!("nodeinnet", "cannot start the VPN: {e}"))
        .is_ok()
}

pub fn stop_vpn(window: &gtk::Window) {
    let stopped = with_activity(window, |env, activity| {
        let vpn = app_class(env, activity, "net.nodeinnet.gtk.Vpn")?;
        env.call_static_method(
            &vpn,
            "stop",
            "(Landroid/content/Context;)V",
            &[JValue::Object(activity)],
        )?;
        Ok(())
    });
    if let Err(e) = stopped {
        gtk::glib::g_warning!("nodeinnet", "cannot stop the VPN: {e}");
    }
}

pub fn launcher_packages(window: &gtk::Window) -> Vec<String> {
    let listed = with_activity(window, |env, activity| {
        let vpn = app_class(env, activity, "net.nodeinnet.gtk.Vpn")?;
        let array = env
            .call_static_method(
                &vpn,
                "launcherPackages",
                "(Landroid/content/Context;)[Ljava/lang/String;",
                &[JValue::Object(activity)],
            )?
            .l()?;
        strings(env, &JObjectArray::from(array))
    });
    listed.unwrap_or_else(|e| {
        gtk::glib::g_warning!("nodeinnet", "cannot list the apps: {e}");
        Vec::new()
    })
}

fn strings(env: &mut JNIEnv, array: &JObjectArray) -> JniResult<Vec<String>> {
    let len = env.get_array_length(array)?;
    let mut items = Vec::with_capacity(len as usize);
    for i in 0..len {
        let item = JString::from(env.get_object_array_element(array, i)?);
        items.push(String::from(env.get_string(&item)?));
        env.delete_local_ref(item)?;
    }
    Ok(items)
}

pub fn launcher_apps(window: &gtk::Window) -> Vec<(String, String)> {
    let listed = with_activity(window, |env, activity| {
        let vpn = app_class(env, activity, "net.nodeinnet.gtk.Vpn")?;
        let flat = JObjectArray::from(
            env.call_static_method(
                &vpn,
                "launcherApps",
                "(Landroid/content/Context;)[Ljava/lang/String;",
                &[JValue::Object(activity)],
            )?
            .l()?,
        );
        let items = strings(env, &flat)?;
        Ok(items
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[package, label]| (package.clone(), label.clone()))
            .collect())
    });
    listed.unwrap_or_else(|e| {
        gtk::glib::g_warning!("nodeinnet", "cannot list the apps: {e}");
        Vec::new()
    })
}

/// One of this app's classes: FindClass on GTK's thread sees only the system's.
fn app_class<'local>(
    env: &mut JNIEnv<'local>,
    activity: &JObject,
    name: &str,
) -> JniResult<JClass<'local>> {
    let loader = env
        .call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
        .l()?;
    let name = env.new_string(name)?;
    let class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )?
        .l()?;
    Ok(JClass::from(class))
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
