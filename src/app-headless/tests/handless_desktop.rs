use futures_util::StreamExt;
use nodeinnet_p2p::P2pMessage;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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

fn test_frame() -> Vec<u8> {
    let mut f = Vec::with_capacity(320 * 180 * 4);
    for _ in 0..320 * 180 {
        f.extend_from_slice(&[10, 200, 60, 255]);
    }
    f
}

#[test]
fn desktop_session_over_rest_with_synthetic_media() {
    let sent: Arc<Mutex<Vec<P2pMessage>>> = Arc::new(Mutex::new(Vec::new()));
    let sent_for_app = sent.clone();

    let h = app_headless::start(move || {
        let mut app = app_headless::App::new();
        let router = app.desktop.router();
        let streaming = Arc::new(AtomicBool::new(false));
        let sent = sent_for_app;
        app.desktop.set_resource(
            "b-screen",
            move |msg| sent.lock().unwrap().push(msg),
            move |on, _opts| {
                streaming.store(on, Ordering::Relaxed);
                if on {
                    let router = router.clone();
                    let streaming = streaming.clone();
                    let frame = test_frame();
                    tokio::spawn(async move {
                        while streaming.load(Ordering::Relaxed) {
                            router.record_frame(320, 180, 1000, &frame);
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    });
                }
            },
        );
        app
    });
    let base = format!("http://127.0.0.1:{}", h.port);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (mut events, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/api/events", h.port))
                .await
                .expect("connect events ws");
        let http = reqwest::Client::new();

        http.post(format!("{base}/api/desktop/connect"))
            .json(&json!({ "connect": true }))
            .send()
            .await
            .unwrap();
        wait_event(&mut events, "connected", |v| {
            v["event"] == "desktop_changed" && v["desktop"]["connected"] == true
        })
        .await;

        let stats = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let s: Value = http
                    .get(format!("{base}/api/desktop/stats"))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                if s["frames_total"].as_u64().unwrap_or(0) >= 5 {
                    return s;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .expect("frames never arrived");
        assert!(
            stats["time_to_first_frame_ms"].as_u64().unwrap() < 3000,
            "TTFF budget"
        );
        assert_eq!(stats["last_width"], 320);
        assert_eq!(stats["last_height"], 180);
        assert!(
            stats["avg_fps"].as_f64().unwrap() > 10.0,
            "fps floor; got {}",
            stats["avg_fps"]
        );
        assert_eq!(stats["stream_stops"], 0);

        let png_bytes = http
            .get(format!("{base}/api/desktop/frame"))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(&png_bytes[..]));
        let mut reader = decoder.read_info().expect("valid png");
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("decode png");
        assert_eq!((info.width, info.height), (320, 180));
        assert_eq!(
            &buf[..4],
            &[60, 200, 10, 255],
            "BGRA correctly swapped to RGBA"
        );

        let input = json!({ "event": { "MouseMove": { "x": 5, "y": 7 } } });
        http.post(format!("{base}/api/desktop/input"))
            .json(&input)
            .send()
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !sent
                .lock()
                .unwrap()
                .iter()
                .any(|m| matches!(m, P2pMessage::RemoteDesktopInput { .. })),
            "input before control must be dropped"
        );

        http.post(format!("{base}/api/desktop/control"))
            .json(&json!({ "enabled": true }))
            .send()
            .await
            .unwrap();
        wait_event(&mut events, "controlling", |v| {
            v["event"] == "desktop_changed" && v["desktop"]["controlling"] == true
        })
        .await;
        http.post(format!("{base}/api/desktop/input"))
            .json(&input)
            .send()
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if sent.lock().unwrap().iter().any(|m| {
                    matches!(
                        m,
                        P2pMessage::RemoteDesktopInput {
                            resource_id,
                            event: nodeinnet_p2p::DesktopInputEvent::MouseMove { x: 5, y: 7 }
                        } if resource_id == "b-screen"
                    )
                }) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("controlled input must reach the transport");

        http.post(format!("{base}/api/desktop/connect"))
            .json(&json!({ "connect": false }))
            .send()
            .await
            .unwrap();
        wait_event(&mut events, "disconnected", |v| {
            v["event"] == "desktop_changed"
                && v["desktop"]["connected"] == false
                && v["desktop"]["controlling"] == false
        })
        .await;
        let s: Value = http
            .get(format!("{base}/api/desktop/stats"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(s["stream_stops"], 1);

        let r = http
            .post(format!("{base}/api/desktop/input"))
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
    });
}
