#[cfg(not(windows))]
use gtk::prelude::*;
use std::path::PathBuf;

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
            if let Some(path) = res.ok().and_then(|f| f.path()) {
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
            if let Some(path) = res.ok().and_then(|f| f.path()) {
                on_pick(path);
            }
        });
    }
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
