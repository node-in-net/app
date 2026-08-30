use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

static EXE_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn init_exe_path() {
    if let Ok(exe_path) = std::env::current_exe() {
        let _ = EXE_PATH.set(exe_path);
    }
}

pub fn restart_app() {
    let Some(exe_path) = EXE_PATH
        .get()
        .cloned()
        .or_else(|| std::env::current_exe().ok())
    else {
        std::process::exit(0);
    };

    #[cfg_attr(target_os = "windows", allow(unused_mut))]
    let mut exe_str = exe_path.to_string_lossy().into_owned();

    #[cfg(not(target_os = "windows"))]
    if let Some(stripped) = exe_str.strip_suffix(" (deleted)") {
        exe_str = stripped.to_string();
    }

    #[cfg(target_os = "windows")]
    let _ = Command::new(&exe_str).spawn();

    #[cfg(target_os = "macos")]
    {
        let bundle = exe_str
            .find(".app/Contents/MacOS/")
            .map(|pos| &exe_str[..pos + 4]);
        let spawned = bundle.is_some_and(|b| Command::new("open").arg("-n").arg(b).spawn().is_ok());
        if !spawned {
            let _ = detached(&exe_str);
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = detached(&exe_str);

    std::process::exit(0);
}

#[cfg(not(target_os = "windows"))]
fn detached(exe: &str) -> std::io::Result<std::process::Child> {
    Command::new(exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}
