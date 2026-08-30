use app_core::fm::{ResourceTab, Side};
use app_core::fm_core::rpc::FileSystemRpc;
use app_core::local_fs::LocalFsRpc;
use app_core::workspace::DeviceInfo;
use app_headless::App;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("app-headless-fm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    std::fs::write(dir.join("readme.md"), b"hello").unwrap();
    std::fs::write(dir.join("docs/notes.md"), b"notes").unwrap();
    dir
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn wait_event(ws: &mut Ws, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let msg = ws
                .next()
                .await
                .expect("events ws closed")
                .expect("ws error");
            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                let v: Value = serde_json::from_str(&text).expect("event is json");
                if pred(&v) {
                    return v;
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for event: {what}"))
}

fn panel_names(ev: &Value) -> Vec<String> {
    ev["panel"]["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn fm_over_rest_drives_the_real_disk() {
    let dir = scratch();
    let dir_for_app = dir.clone();

    let h = app_headless::start(move || {
        let mut app = App::new();
        app.workspace.apply_snapshot(vec![DeviceInfo {
            id: "local".into(),
            name: "This machine".into(),
            os: "linux".into(),
            online: true,
            link: app_core::workspace::LinkState::Connected,
            services: Vec::new(),
        }]);
        app.fm.set_resources(
            Side::Right,
            vec![(
                ResourceTab {
                    id: "scratch".into(),
                    label: "Scratch".into(),
                },
                Rc::new(LocalFsRpc::new(dir_for_app)) as Rc<dyn FileSystemRpc>,
            )],
        );
        app
    });
    let base = format!("http://127.0.0.1:{}", h.port);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/api/events", h.port))
                .await
                .expect("connect events ws");
        let http = reqwest::Client::new();
        let post = |path: &str, body: Value| {
            let http = http.clone();
            let path = path.to_string();
            let url = format!("{base}{path}");
            async move {
                let r = http.post(url).json(&body).send().await.expect("post");
                assert!(r.status().is_success(), "POST {path} -> {}", r.status());
            }
        };

        post("/api/panel/right/resource", json!({ "index": 0 })).await;
        let ev = wait_event(&mut ws, "root listing", |v| {
            v["event"] == "panel_updated"
                && v["side"] == "right"
                && !v["panel"]["entries"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .is_empty()
        })
        .await;
        assert_eq!(panel_names(&ev), vec!["docs", "readme.md"]);
        assert_eq!(ev["panel"]["cwd"], "/");

        post("/api/panel/right/mkdir", json!({ "name": "made-by-rest" })).await;
        let ev = wait_event(&mut ws, "mkdir listing", |v| {
            v["event"] == "panel_updated" && panel_names(v).contains(&"made-by-rest".to_string())
        })
        .await;
        assert_eq!(panel_names(&ev), vec!["docs", "made-by-rest", "readme.md"]);
        assert!(
            dir.join("made-by-rest").is_dir(),
            "dir exists on the real disk"
        );

        post("/api/panel/right/cursor", json!({ "index": 0 })).await;
        post("/api/panel/right/enter", json!({})).await;
        let ev = wait_event(&mut ws, "entered /docs", |v| {
            v["event"] == "panel_updated" && v["panel"]["cwd"] == "/docs"
        })
        .await;
        assert_eq!(panel_names(&ev), vec!["notes.md"]);
        assert_eq!(ev["panel"]["can_back"], true);

        let st: Value = http
            .get(format!("{base}/api/panel/right/state"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(st["cwd"], "/docs");
        assert_eq!(st["resources"][0]["label"], "Scratch");

        post("/api/device/select", json!({ "id": "local" })).await;
        wait_event(&mut ws, "selection", |v| {
            v["event"] == "selection_changed" && v["device"] == "local"
        })
        .await;
        post("/api/service/open", json!({ "kind": "files" })).await;
        post("/api/service/open", json!({ "kind": "terminal" })).await;
        wait_event(&mut ws, "services", |v| v["event"] == "services_changed").await;

        let full: Value = http
            .get(format!("{base}/api/state"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(full["workspace"]["selected"], "local");
        assert_eq!(
            full["workspace"]["panes"]["open"],
            json!(["files", "terminal"])
        );
        assert_eq!(full["workspace"]["panes"]["focused"], "terminal");
        assert_eq!(full["right"]["cwd"], "/docs");

        let r = http
            .post(format!("{base}/api/panel/right/mkdir"))
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
    });

    let _ = std::fs::remove_dir_all(&dir);
}
