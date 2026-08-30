use app_core::fm::{ResourceTab, Side};
use app_core::fm_core::rpc::FileSystemRpc;
use app_core::local_fs::LocalFsRpc;
use app_core::session::AuthRpc;
use app_core::terminal::TerminalRouter;
use app_core::workspace::DeviceInfo;
use nodeinnet_p2p::P2pMessage;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

struct AcceptAnyAuth;

#[async_trait::async_trait(?Send)]
impl AuthRpc for AcceptAnyAuth {
    async fn login(&self, _login: String, _password: String) -> Result<(), String> {
        Ok(())
    }
    async fn register_device(&self, _node_id: String, _device_name: String) -> Result<(), String> {
        Ok(())
    }
}

fn local_pty_transport(router: Arc<TerminalRouter>) -> impl Fn(P2pMessage) {
    type Live = (
        tokio::sync::mpsc::Sender<Vec<u8>>,
        tokio::sync::mpsc::Sender<(u16, u16)>,
    );
    let session: Rc<RefCell<Option<Live>>> = Rc::new(RefCell::new(None));

    move |msg| match msg {
        P2pMessage::StartTerminal { resource_id } => {
            match node_functions::terminal::spawn_pty_session(None) {
                Ok(pty) => {
                    let node_functions::terminal::PtySession {
                        input_tx,
                        mut output_rx,
                        resize_tx,
                    } = pty;
                    *session.borrow_mut() = Some((input_tx, resize_tx));
                    let router = router.clone();
                    tokio::spawn(async move {
                        while let Some(data) = output_rx.recv().await {
                            let _ = router.route(P2pMessage::TerminalOutput {
                                resource_id: resource_id.clone(),
                                data,
                            });
                        }
                    });
                }
                Err(e) => {
                    let _ = router.route(P2pMessage::TerminalOutput {
                        resource_id,
                        data: format!("failed to spawn shell: {e}\r\n").into_bytes(),
                    });
                }
            }
        }
        P2pMessage::TerminalInput { data, .. } => {
            if let Some((input, _)) = &*session.borrow() {
                let _ = input.try_send(data);
            }
        }
        P2pMessage::TerminalResize { rows, cols, .. } => {
            if let Some((_, resize)) = &*session.borrow() {
                let _ = resize.try_send((rows, cols));
            }
        }
        P2pMessage::StopTerminal { .. } => {
            session.borrow_mut().take();
        }
        _ => {}
    }
}

fn sandbox() -> PathBuf {
    let dir = std::env::temp_dir().join("remake-playground");
    let _ = std::fs::create_dir_all(dir.join("docs"));
    let _ = std::fs::write(
        dir.join("readme.md"),
        b"This sandbox is the RIGHT file-manager pane of the remake playground.\n",
    );
    let _ = std::fs::write(
        dir.join("docs/notes.md"),
        b"mutations over the API land here\n",
    );
    dir
}

fn main() {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/".into());
    let sandbox_dir = sandbox();
    let sandbox_str = sandbox_dir.display().to_string();
    let user = std::env::var("USER").unwrap_or_else(|_| "dev".into());

    let home_for_app = home.clone();
    let sandbox_for_app = sandbox_dir.clone();
    let h = app_headless::start(move || {
        let mut app = app_headless::App::new();

        app.session.set_rpc(Rc::new(AcceptAnyAuth));
        app.session
            .begin_setup("playground-node".into(), format!("{user}'s machine"));

        app.workspace.apply_snapshot(vec![DeviceInfo {
            id: "this-machine".into(),
            name: "This machine".into(),
            os: std::env::consts::OS.into(),
            online: true,
            link: app_core::workspace::LinkState::Connected,
            services: Vec::new(),
        }]);

        app.fm.set_resources(
            Side::Left,
            vec![(
                ResourceTab {
                    id: "home".into(),
                    label: "Home".into(),
                },
                Rc::new(LocalFsRpc::new(home_for_app)) as Rc<dyn FileSystemRpc>,
            )],
        );
        app.fm.set_resources(
            Side::Right,
            vec![(
                ResourceTab {
                    id: "sandbox".into(),
                    label: "Sandbox".into(),
                },
                Rc::new(LocalFsRpc::new(sandbox_for_app)) as Rc<dyn FileSystemRpc>,
            )],
        );

        let router = app.terminal.router();
        app.terminal
            .set_resource("local-shell", local_pty_transport(router));

        app
    });

    let base = format!("http://127.0.0.1:{}", h.port);
    println!(
        r#"
Node.In.Net remake — headless playground
=========================================
  API base:  {base}
  events:    ws://127.0.0.1:{port}/api/events
  terminal:  ws://127.0.0.1:{port}/api/terminal/ws   (raw byte pipe to a REAL local shell)

  Left FM pane  = {home}
  Right FM pane = {sandbox_str}   (safe to mutate)

Try the wizard (concept board frames 1-4):
  curl {base}/api/auth/state
  curl -X POST {base}/api/setup/name -d '{{"name":"My Mac"}}'
  curl -X POST {base}/api/auth/login -d '{{"login":"demo","password":"demo"}}'

Workspace + files:
  curl -X POST {base}/api/device/select  -d '{{"id":"this-machine"}}'
  curl -X POST {base}/api/service/open   -d '{{"kind":"files"}}'
  curl -X POST {base}/api/panel/left/resource  -d '{{"index":0}}'
  curl {base}/api/panel/left/state          # ← your real home dir
  curl -X POST {base}/api/panel/right/resource -d '{{"index":0}}'
  curl -X POST {base}/api/panel/right/mkdir    -d '{{"name":"made-by-hand"}}'
  ls {sandbox_str}                          # ← it is really there

Terminal (needs a WS client, e.g. websocat):
  curl -X POST {base}/api/terminal/start
  websocat -b ws://127.0.0.1:{port}/api/terminal/ws   # then type: ls

Full state anytime:  curl {base}/api/state
Ctrl-C stops the playground.
"#,
        port = h.port,
    );

    loop {
        std::thread::park();
    }
}
