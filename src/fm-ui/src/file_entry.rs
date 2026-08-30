use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;

mod imp {
    use super::*;
    use gtk::glib::subclass::prelude::*;

    #[derive(Default)]
    pub struct FileEntry {
        pub name: RefCell<String>,
        pub path: RefCell<String>,
        pub is_dir: RefCell<bool>,
        pub size: RefCell<u64>,
        pub date: RefCell<String>,
        pub permissions: RefCell<Option<u32>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileEntry {
        const NAME: &'static str = "FmFileEntry";
        type Type = super::FileEntry;
    }

    impl ObjectImpl for FileEntry {
        fn properties() -> &'static [glib::ParamSpec] {
            use std::sync::OnceLock;
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
                vec![
                    glib::ParamSpecString::builder("name").build(),
                    glib::ParamSpecString::builder("path").build(),
                    glib::ParamSpecBoolean::builder("is-dir").build(),
                    glib::ParamSpecUInt64::builder("size").build(),
                    glib::ParamSpecString::builder("date").build(),
                    glib::ParamSpecUInt::builder("permissions").build(),
                ]
            })
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "name" => {
                    let name = value.get().expect("type error");
                    self.name.replace(name);
                }
                "path" => {
                    let path = value.get().expect("type error");
                    self.path.replace(path);
                }
                "is-dir" => {
                    let is_dir = value.get().expect("type error");
                    self.is_dir.replace(is_dir);
                }
                "size" => {
                    let size = value.get().expect("type error");
                    self.size.replace(size);
                }
                "date" => {
                    let date = value.get().expect("type error");
                    self.date.replace(date);
                }
                "permissions" => {
                    let perms: u32 = value.get().expect("type error");
                    if perms == u32::MAX {
                        self.permissions.replace(None);
                    } else {
                        self.permissions.replace(Some(perms));
                    }
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "name" => self.name.borrow().to_value(),
                "path" => self.path.borrow().to_value(),
                "is-dir" => self.is_dir.borrow().to_value(),
                "size" => self.size.borrow().to_value(),
                "date" => self.date.borrow().to_value(),
                "permissions" => {
                    let perms = self.permissions.borrow().unwrap_or(u32::MAX);
                    perms.to_value()
                }
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct FileEntry(ObjectSubclass<imp::FileEntry>);
}

impl FileEntry {
    pub fn new(
        name: &str,
        path: &str,
        is_dir: bool,
        size: u64,
        date: &str,
        permissions: Option<u32>,
    ) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("path", path)
            .property("is-dir", is_dir)
            .property("size", size)
            .property("date", date)
            .property("permissions", permissions.unwrap_or(u32::MAX))
            .build()
    }

    pub fn name(&self) -> String {
        self.property("name")
    }

    pub fn path(&self) -> String {
        self.property("path")
    }

    pub fn is_dir(&self) -> bool {
        self.property("is-dir")
    }

    pub fn size(&self) -> u64 {
        self.property("size")
    }

    pub fn date(&self) -> String {
        self.property("date")
    }

    pub fn permissions(&self) -> Option<u32> {
        let val: u32 = self.property("permissions");
        if val == u32::MAX {
            None
        } else {
            Some(val)
        }
    }

    pub fn set_permissions(&self, perms: Option<u32>) {
        self.set_property("permissions", perms.unwrap_or(u32::MAX));
    }
}
