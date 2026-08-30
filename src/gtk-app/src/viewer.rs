use app_core::fm::Side;
use app_headless::ApiCmd;
use common::AppError;
use fm_core::rpc::FileSystemRpc;
use gtk::prelude::*;
use std::rc::Rc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

pub struct PanelFs {
    side: Side,
    cmd: UnboundedSender<ApiCmd>,
    local: bool,
    read_only: bool,
}

impl PanelFs {
    pub async fn probe(side: Side, cmd: UnboundedSender<ApiCmd>) -> Option<Self> {
        let (tx, rx) = oneshot::channel();
        cmd.send(ApiCmd::FmProviderInfo { side, reply: tx }).ok()?;
        let (local, read_only) = rx.await.ok()??;
        Some(Self {
            side,
            cmd,
            local,
            read_only,
        })
    }
}

#[async_trait::async_trait(?Send)]
impl FileSystemRpc for PanelFs {
    fn is_local(&self) -> bool {
        self.local
    }

    fn is_read_only(&self) -> bool {
        self.read_only
    }

    async fn list_dir(&self, path: String) -> Result<Vec<fm_core::rpc::RemoteFileEntry>, AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd
            .send(ApiCmd::FmListDir {
                side: self.side,
                path,
                reply: tx,
            })
            .map_err(|_| AppError::Other("the core loop is gone".into()))?;
        rx.await
            .map_err(|_| AppError::Other("the core loop dropped the listing".into()))?
            .map_err(AppError::Other)
    }

    async fn read_file(
        &self,
        path: String,
        _progress_callback: Option<Box<dyn Fn(u64) + 'static>>,
    ) -> Result<Vec<u8>, AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd
            .send(ApiCmd::FmReadFile {
                side: self.side,
                path,
                reply: tx,
            })
            .map_err(|_| AppError::Other("the core loop is gone".into()))?;
        rx.await
            .map_err(|_| AppError::Other("the core loop dropped the read".into()))?
            .map_err(AppError::Other)
    }

    async fn write_file(
        &self,
        path: String,
        content: Vec<u8>,
        _permissions: Option<u32>,
        _progress_callback: Option<Box<dyn Fn(u64) + 'static>>,
    ) -> Result<(), AppError> {
        let (tx, rx) = oneshot::channel();
        self.cmd
            .send(ApiCmd::FmWriteFile {
                side: self.side,
                path,
                content,
                reply: tx,
            })
            .map_err(|_| AppError::Other("the core loop is gone".into()))?;
        rx.await
            .map_err(|_| AppError::Other("the core loop dropped the write".into()))?
            .map_err(AppError::Other)
    }
}

fn services(cmd: UnboundedSender<ApiCmd>, side: Side, cwd: String) -> gtk_viewer_ui::HostServices {
    gtk_viewer_ui::HostServices {
        current_dir: Rc::new(move || cwd.clone()),
        on_saved: Rc::new(move || {
            let _ = cmd.send(ApiCmd::FmRefresh { side });
        }),
        ..Default::default()
    }
}

pub fn open_file(
    parent: &impl IsA<gtk::Window>,
    cmd: UnboundedSender<ApiCmd>,
    side: Side,
    path: String,
    cwd: String,
    start_in_edit_mode: bool,
) {
    let parent = parent.clone().upcast::<gtk::Window>();
    gtk::glib::spawn_future_local(async move {
        let Some(fs) = PanelFs::probe(side, cmd.clone()).await else {
            eprintln!("[viewer] ✗ this panel has no filesystem to read from");
            return;
        };
        let name = path
            .rsplit(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&path)
            .to_string();
        gtk_viewer_ui::open(
            &parent,
            Box::new(gtk_viewer_ui::TextPlugin),
            path,
            name,
            Rc::new(fs),
            services(cmd, side, cwd),
            start_in_edit_mode,
        );
    });
}

pub fn new_file(
    parent: &impl IsA<gtk::Window>,
    cmd: UnboundedSender<ApiCmd>,
    side: Side,
    cwd: String,
) {
    let parent = parent.clone().upcast::<gtk::Window>();
    gtk::glib::spawn_future_local(async move {
        let Some(fs) = PanelFs::probe(side, cmd.clone()).await else {
            eprintln!("[viewer] ✗ this panel has no filesystem to write to");
            return;
        };
        if fs.is_read_only() {
            eprintln!("[viewer] ✗ this panel's filesystem refuses writes");
            return;
        }
        gtk_viewer_ui::open(
            &parent,
            Box::new(gtk_viewer_ui::NewFilePlugin),
            String::new(),
            crate::i18n::tr("files.new_file"),
            Rc::new(fs),
            services(cmd, side, cwd),
            true,
        );
    });
}
