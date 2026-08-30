use futures_util::{SinkExt, StreamExt};
use nodeinnet_p2p::P2pMessage;
use serde_json::{json, Value};
use std::time::Duration;

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

async fn wait_bytes(ws: &mut Ws, needle: &[u8]) -> Vec<u8> {
    let mut acc: Vec<u8> = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let msg = ws
                .next()
                .await
                .expect("terminal ws closed")
                .expect("ws error");
            if let tokio_tungstenite::tungstenite::Message::Binary(b) = msg {
                acc.extend_from_slice(&b);
                if acc.windows(needle.len()).any(|w| w == needle) {
                    return acc.clone();
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "timed out waiting for terminal bytes {:?}; got {:?}",
            String::from_utf8_lossy(needle),
            String::from_utf8_lossy(&acc)
        )
    })
}

#[test]
fn terminal_pipe_over_rest_and_ws() {
    let h = app_headless::start(|| {
        let mut app = app_headless::App::new();
        let router = app.terminal.router();
        app.terminal.set_resource("term-1", move |msg| {
            if let P2pMessage::TerminalInput { resource_id, data } = msg {
                let _ = router.route(P2pMessage::TerminalOutput { resource_id, data });
            }
        });
        app
    });
    let base = format!("http://127.0.0.1:{}", h.port);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (mut events, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/api/events", h.port))
                .await
                .expect("connect events ws");
        let (mut pipe, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/api/terminal/ws", h.port))
                .await
                .expect("connect terminal ws");
        let http = reqwest::Client::new();

        let r = http
            .post(format!("{base}/api/terminal/start"))
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
        let ev = wait_event(&mut events, "terminal alive", |v| {
            v["event"] == "terminal_changed" && v["terminal"]["alive"] == true
        })
        .await;
        assert_eq!(ev["terminal"]["resource_id"], "term-1");

        pipe.send(tokio_tungstenite::tungstenite::Message::Text(
            "hello-pipe".into(),
        ))
        .await
        .unwrap();
        wait_bytes(&mut pipe, b"hello-pipe").await;

        let r = http
            .post(format!("{base}/api/terminal/resize"))
            .json(&json!({ "rows": 50, "cols": 120 }))
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
        wait_event(&mut events, "resized", |v| {
            v["event"] == "terminal_changed" && v["terminal"]["rows"] == 50
        })
        .await;

        let r = http
            .post(format!("{base}/api/terminal/resize"))
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);

        http.post(format!("{base}/api/terminal/stop"))
            .send()
            .await
            .unwrap();
        wait_event(&mut events, "stopped", |v| {
            v["event"] == "terminal_changed" && v["terminal"]["alive"] == false
        })
        .await;
        let st: Value = http
            .get(format!("{base}/api/state"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(st["terminal"]["alive"], false);
        assert_eq!(st["terminal"]["cols"], 120);
    });
}
