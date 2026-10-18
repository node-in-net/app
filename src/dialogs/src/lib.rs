#[cfg(not(windows))]
use gtk::prelude::*;
use std::path::PathBuf;

mod alert;
pub mod gate;
pub mod im;
pub mod menu;
mod touch;

pub use alert::Alert;
pub use touch::{set_touch, touch};

pub struct Filter {
    pub name: String,
    pub extensions: Vec<String>,
}

impl Filter {
    pub fn new(name: &str, extensions: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
        }
    }
}

#[cfg_attr(windows, allow(unused_variables))]
pub fn select_folder<F>(parent: Option<&gtk::Window>, title: &str, on_pick: F)
where
    F: Fn(PathBuf) + 'static,
{
    #[cfg(windows)]
    native_pick(title.to_string(), None, true, on_pick);

    #[cfg(not(windows))]
    {
        let dialog = gtk::FileDialog::builder().title(title).modal(true).build();
        let keep_alive = dialog.clone();
        dialog.select_folder(parent, gtk::gio::Cancellable::NONE, move |res| {
            let _keep_alive = keep_alive;
            if let Some(path) = res.ok().and_then(|f| local_folder(&f)) {
                on_pick(path);
            }
        });
    }
}

#[cfg_attr(windows, allow(unused_variables))]
pub fn open_file<F>(parent: Option<&gtk::Window>, title: &str, filter: Option<Filter>, on_pick: F)
where
    F: Fn(PathBuf) + 'static,
{
    #[cfg(windows)]
    native_pick(title.to_string(), filter, false, on_pick);

    #[cfg(not(windows))]
    {
        let builder = gtk::FileDialog::builder().title(title).modal(true);
        let builder = match filter {
            Some(f) => {
                let ff = gtk::FileFilter::new();
                ff.set_name(Some(&f.name));
                for ext in &f.extensions {
                    ff.add_pattern(&format!("*.{ext}"));
                }
                let list = gtk::gio::ListStore::new::<gtk::FileFilter>();
                list.append(&ff);
                builder.filters(&list).default_filter(&ff)
            }
            None => builder,
        };
        let dialog = builder.build();
        let keep_alive = dialog.clone();
        dialog.open(parent, gtk::gio::Cancellable::NONE, move |res| {
            let _keep_alive = keep_alive;
            let Ok(file) = res else { return };
            if file.uri_scheme().as_deref() != Some("content") {
                if let Some(path) = file.path() {
                    on_pick(path);
                }
                return;
            }
            gtk::glib::spawn_future_local(async move {
                if let Some(path) = copy_to_cache(&file).await {
                    on_pick(path);
                }
            });
        });
    }
}

/// A picked folder as a real path. On Android the picker hands out a SAF tree URI; one on shared
/// storage maps to its path (the app has All-files access), anything else cannot be shared.
#[cfg(not(windows))]
fn local_folder(folder: &gtk::gio::File) -> Option<PathBuf> {
    if folder.uri_scheme().as_deref() != Some("content") {
        return folder.path();
    }
    let uri = folder.uri();
    let rest = uri.strip_prefix("content://com.android.externalstorage.documents/tree/")?;
    let id = gtk::glib::Uri::unescape_string(rest.split('/').next()?, None::<&str>)?;
    let (volume, rel) = id.split_once(':')?;
    let base = match volume {
        "primary" => std::env::var_os("NODEINNET_STORAGE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/storage/emulated/0")),
        _ => PathBuf::from("/storage").join(volume),
    };
    let path = base.join(rel);
    path.is_dir().then_some(path)
}

/// A picked SAF document has no path: it is copied into the cache under its display name.
#[cfg(not(windows))]
async fn copy_to_cache(file: &gtk::gio::File) -> Option<PathBuf> {
    let info = file
        .query_info_future(
            "standard::display-name",
            gtk::gio::FileQueryInfoFlags::NONE,
            gtk::glib::Priority::DEFAULT,
        )
        .await
        .ok()?;
    // The provider names the file: keep only a plain last component of it.
    let name = info.display_name();
    let name = std::path::Path::new(&name).file_name()?.to_owned();
    let unique = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let dir = gtk::glib::user_cache_dir().join("picked").join(unique);
    std::fs::create_dir_all(&dir).ok()?;
    let dest = dir.join(name);
    let (copy, _progress) = file.copy_future(
        &gtk::gio::File::for_path(&dest),
        gtk::gio::FileCopyFlags::OVERWRITE,
        gtk::glib::Priority::DEFAULT,
    );
    copy.await.ok()?;
    Some(dest)
}

#[cfg(windows)]
fn native_pick<F>(title: String, filter: Option<Filter>, folder: bool, on_pick: F)
where
    F: Fn(PathBuf) + 'static,
{
    let (tx, rx) = relm4::channel::<PathBuf>();

    gtk::glib::spawn_future_local(async move {
        if let Some(path) = rx.recv().await {
            on_pick(path);
        }
    });

    std::thread::spawn(move || {
        let mut dialog = rfd::FileDialog::new().set_title(&title);
        if let Some(f) = filter {
            let exts: Vec<&str> = f.extensions.iter().map(|e| e.as_str()).collect();
            dialog = dialog.add_filter(&f.name, &exts);
        }
        let picked = if folder {
            dialog.pick_folder()
        } else {
            dialog.pick_file()
        };
        if let Some(path) = picked {
            let _ = tx.send(path);
        }
    });
}
