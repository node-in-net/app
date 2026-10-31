use app_core::webvpn::{StreamEvent, WebVpnRouter};
use client_core::NetCmd;
use nodeinnet_p2p::P2pMessage;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

const PORT_BASE: u16 = 54145;
const PORT_SPAN: u16 = 100;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// For the FIN to the app; a stream whose app never answers it is given up.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

pub struct SocksProxy {
    port: u16,
    stop: Option<oneshot::Sender<()>>,
}

impl SocksProxy {
    pub fn start(
        resource_id: String,
        peer_id: String,
        net_tx: mpsc::Sender<NetCmd>,
        router: Arc<WebVpnRouter>,
    ) -> std::io::Result<Self> {
        let (port, listeners) = bind_loopback()?;
        let (stop_tx, stop_rx) = oneshot::channel();
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Runtime::new() else {
                return;
            };
            rt.block_on(run(
                listeners,
                resource_id,
                peer_id,
                net_tx,
                router,
                stop_rx,
            ));
        });
        Ok(Self {
            port,
            stop: Some(stop_tx),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stop(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for SocksProxy {
    fn drop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

fn bind_loopback() -> std::io::Result<(u16, Vec<std::net::TcpListener>)> {
    let mut last = std::io::Error::new(std::io::ErrorKind::AddrInUse, "no free loopback port");
    for port in PORT_BASE..PORT_BASE.saturating_add(PORT_SPAN) {
        let v4 = match std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))) {
            Ok(l) => l,
            Err(e) => {
                last = e;
                continue;
            }
        };
        match std::net::TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port))) {
            Ok(v6) => return Ok((port, vec![v4, v6])),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::AddrNotAvailable | std::io::ErrorKind::Unsupported
                ) =>
            {
                return Ok((port, vec![v4]));
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

async fn run(
    listeners: Vec<std::net::TcpListener>,
    resource_id: String,
    peer_id: String,
    net_tx: mpsc::Sender<NetCmd>,
    router: Arc<WebVpnRouter>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let (accept_tx, mut accept_rx) = mpsc::channel::<tokio::net::TcpStream>(32);
    for std_listener in listeners {
        if std_listener.set_nonblocking(true).is_err() {
            continue;
        }
        let Ok(listener) = tokio::net::TcpListener::from_std(std_listener) else {
            continue;
        };
        let tx = accept_tx.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                if tx.send(stream).await.is_err() {
                    break;
                }
            }
        });
    }
    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            Some(stream) = accept_rx.recv() => {
                let resource_id = resource_id.clone();
                let peer_id = peer_id.clone();
                let net_tx = net_tx.clone();
                let router = router.clone();
                tokio::spawn(async move {
                    let _ = handle_conn(stream, resource_id, peer_id, net_tx, router).await;
                });
            }
        }
    }
}

async fn handle_conn(
    mut stream: tokio::net::TcpStream,
    resource_id: String,
    peer_id: String,
    net_tx: mpsc::Sender<NetCmd>,
    router: Arc<WebVpnRouter>,
) -> std::io::Result<()> {
    let mut greeting = [0u8; 2];
    stream.read_exact(&mut greeting).await?;
    if greeting[0] != 0x05 {
        return Ok(());
    }
    let mut methods = vec![0u8; greeting[1] as usize];
    stream.read_exact(&mut methods).await?;
    stream.write_all(&[0x05, 0x00]).await?;

    let mut req = [0u8; 4];
    stream.read_exact(&mut req).await?;
    if req[0] != 0x05 || req[1] != 0x01 {
        stream
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await?;
        return Ok(());
    }
    let host = match req[3] {
        0x01 => {
            let mut ip = [0u8; 4];
            stream.read_exact(&mut ip).await?;
            Ipv4Addr::from(ip).to_string()
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            stream.read_exact(&mut domain).await?;
            String::from_utf8_lossy(&domain).into_owned()
        }
        0x04 => {
            let mut ip = [0u8; 16];
            stream.read_exact(&mut ip).await?;
            Ipv6Addr::from(ip).to_string()
        }
        _ => return Ok(()),
    };
    let mut port = [0u8; 2];
    stream.read_exact(&mut port).await?;
    let port = u16::from_be_bytes(port);

    let (stream_id, events) =
        match connect_through_peer(&host, port, &peer_id, &resource_id, &net_tx, &router).await {
            Ok(opened) => opened,
            Err(e) => {
                eprintln!("[net] ✗ {e}");
                stream
                    .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await?;
                return Ok(());
            }
        };
    stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;
    pump(
        stream,
        stream_id,
        events,
        peer_id,
        resource_id,
        net_tx,
        router,
    )
    .await;
    Ok(())
}

pub async fn connect_through_peer(
    host: &str,
    port: u16,
    peer_id: &str,
    resource_id: &str,
    net_tx: &mpsc::Sender<NetCmd>,
    router: &WebVpnRouter,
) -> Result<(Uuid, mpsc::UnboundedReceiver<StreamEvent>), String> {
    let stream_id = Uuid::new_v4();
    let mut events = router.register_stream(stream_id);
    let _ = net_tx
        .send(NetCmd::SendToPeer(
            peer_id.to_string(),
            P2pMessage::SocksConnectRequest {
                resource_id: resource_id.to_string(),
                stream_id,
                host: host.to_string(),
                port,
            },
        ))
        .await;

    let failure = match tokio::time::timeout(CONNECT_TIMEOUT, events.recv()).await {
        Ok(Some(StreamEvent::Connected {
            is_success: true, ..
        })) => return Ok((stream_id, events)),
        // No host: stderr reaches logcat on the phone, and the hosts are the user's browsing.
        Ok(Some(StreamEvent::Connected { error_msg, .. })) => format!(
            "{peer_id} refused a connection: {}",
            error_msg.unwrap_or_else(|| "no reason given".into())
        ),
        Ok(_) => format!("{peer_id} closed a stream before it opened"),
        Err(_) => format!("{peer_id} did not answer a connect"),
    };
    router.close_stream(stream_id);
    Err(failure)
}

/// Both halves stay in the caller's task: the stream is dropped where the caller drops this future.
pub async fn pump<S>(
    stream: S,
    stream_id: Uuid,
    mut events: mpsc::UnboundedReceiver<StreamEvent>,
    peer_id: String,
    resource_id: String,
    net_tx: mpsc::Sender<NetCmd>,
    router: Arc<WebVpnRouter>,
) where
    S: AsyncRead + AsyncWrite + Send,
{
    let (mut rd, mut wr) = tokio::io::split(stream);
    let downstream = async move {
        while let Some(ev) = events.recv().await {
            match ev {
                StreamEvent::Data(d) => {
                    if wr.write_all(&d).await.is_err() {
                        break;
                    }
                }
                StreamEvent::Closed => break,
                StreamEvent::Connected { .. } => {}
            }
        }
        let _ = tokio::time::timeout(SHUTDOWN_GRACE, wr.shutdown()).await;
    };
    let upstream = async {
        let mut buf = [0u8; 16384];
        loop {
            match rd.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    router.count_up(n);
                    if net_tx
                        .send(NetCmd::SendToPeer(
                            peer_id.clone(),
                            P2pMessage::SocksData {
                                resource_id: resource_id.clone(),
                                stream_id,
                                data: buf[..n].to_vec(),
                            },
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        let _ = net_tx
            .send(NetCmd::SendToPeer(
                peer_id.clone(),
                P2pMessage::SocksClose {
                    resource_id: resource_id.clone(),
                    stream_id,
                },
            ))
            .await;
        router.close_stream(stream_id);
    };
    tokio::join!(downstream, upstream);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn socks5_connect_and_data_round_trip() {
        let router = WebVpnRouter::new();
        let (net_tx, mut net_rx) = mpsc::channel::<NetCmd>(64);
        let proxy = SocksProxy::start("res-1".into(), "peer-1".into(), net_tx, router.clone())
            .expect("a loopback port was free");
        let port = proxy.port();

        let peer_router = router.clone();
        tokio::spawn(async move {
            while let Some(NetCmd::SendToPeer(_peer, msg)) = net_rx.recv().await {
                match msg {
                    P2pMessage::SocksConnectRequest {
                        resource_id,
                        stream_id,
                        ..
                    } => {
                        peer_router.route(P2pMessage::SocksConnectResponse {
                            resource_id,
                            stream_id,
                            is_success: true,
                            error_msg: None,
                        });
                    }
                    P2pMessage::SocksData {
                        resource_id,
                        stream_id,
                        data,
                    } => {
                        peer_router.route(P2pMessage::SocksData {
                            resource_id,
                            stream_id,
                            data,
                        });
                    }
                    _ => {}
                }
            }
        });

        let mut c = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .expect("the listener was already bound when start() returned");

        c.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
        let mut sel = [0u8; 2];
        c.read_exact(&mut sel).await.unwrap();
        assert_eq!(sel, [0x05, 0x00]);

        let host = b"example.test";
        let mut req = vec![0x05, 0x01, 0x00, 0x03, host.len() as u8];
        req.extend_from_slice(host);
        req.extend_from_slice(&80u16.to_be_bytes());
        c.write_all(&req).await.unwrap();
        let mut reply = [0u8; 10];
        c.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[0..2], [0x05, 0x00], "CONNECT succeeded");

        c.write_all(b"ping").await.unwrap();
        let mut echo = [0u8; 4];
        c.read_exact(&mut echo).await.unwrap();
        assert_eq!(&echo, b"ping", "data tunnelled and echoed back");

        assert_eq!(router.totals().streams_opened, 1);
    }

    fn echo_peer(
        router: Arc<WebVpnRouter>,
        mut net_rx: mpsc::Receiver<NetCmd>,
    ) -> mpsc::UnboundedReceiver<P2pMessage> {
        let (seen_tx, seen) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(NetCmd::SendToPeer(_peer, msg)) = net_rx.recv().await {
                let _ = seen_tx.send(msg.clone());
                match msg {
                    P2pMessage::SocksConnectRequest {
                        resource_id,
                        stream_id,
                        ..
                    } => {
                        router.route(P2pMessage::SocksConnectResponse {
                            resource_id,
                            stream_id,
                            is_success: true,
                            error_msg: None,
                        });
                    }
                    P2pMessage::SocksData {
                        resource_id,
                        stream_id,
                        data,
                    } => {
                        router.route(P2pMessage::SocksData {
                            resource_id,
                            stream_id,
                            data,
                        });
                    }
                    _ => {}
                }
            }
        });
        seen
    }

    #[tokio::test]
    async fn the_relay_carries_any_byte_stream_not_only_a_socket() {
        let router = WebVpnRouter::new();
        let (net_tx, net_rx) = mpsc::channel::<NetCmd>(64);
        let mut seen = echo_peer(router.clone(), net_rx);

        let (stream_id, events) =
            connect_through_peer("example.test", 80, "peer-1", "res-1", &net_tx, &router)
                .await
                .expect("the peer accepted");
        let (app, ours) = tokio::io::duplex(64 * 1024);
        let relay = tokio::spawn(pump(
            ours,
            stream_id,
            events,
            "peer-1".into(),
            "res-1".into(),
            net_tx,
            router.clone(),
        ));

        let (mut rd, mut wr) = tokio::io::split(app);
        wr.write_all(b"ping").await.unwrap();
        let mut echo = [0u8; 4];
        rd.read_exact(&mut echo).await.unwrap();
        assert_eq!(&echo, b"ping");

        wr.shutdown().await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), relay)
            .await
            .expect("the relay ends when the app closes")
            .unwrap();
        let mut closed = false;
        while let Ok(msg) = seen.try_recv() {
            closed |=
                matches!(msg, P2pMessage::SocksClose { stream_id: id, .. } if id == stream_id);
        }
        assert!(closed, "the peer is told");
        let totals = router.totals();
        assert_eq!((totals.streams_opened, totals.streams_closed), (1, 1));
        assert_eq!((totals.bytes_up, totals.bytes_down), (4, 4));
    }

    #[tokio::test]
    async fn a_refused_connect_leaves_no_stream_behind() {
        let router = WebVpnRouter::new();
        let (net_tx, mut net_rx) = mpsc::channel::<NetCmd>(8);
        let peer = router.clone();
        tokio::spawn(async move {
            while let Some(NetCmd::SendToPeer(_, msg)) = net_rx.recv().await {
                if let P2pMessage::SocksConnectRequest {
                    resource_id,
                    stream_id,
                    ..
                } = msg
                {
                    peer.route(P2pMessage::SocksConnectResponse {
                        resource_id,
                        stream_id,
                        is_success: false,
                        error_msg: Some("on a local network".into()),
                    });
                }
            }
        });

        let err = connect_through_peer("192.168.1.1", 80, "peer-1", "res-1", &net_tx, &router)
            .await
            .unwrap_err();
        assert!(
            err.contains("refused") && err.contains("local network"),
            "{err}"
        );
        let totals = router.totals();
        assert_eq!(totals.errors, 1);
        assert_eq!(totals.streams_opened, totals.streams_closed);
    }

    #[tokio::test]
    async fn two_tunnels_claim_different_ports() {
        let (tx_a, _rx_a) = mpsc::channel::<NetCmd>(1);
        let (tx_b, _rx_b) = mpsc::channel::<NetCmd>(1);
        let a = SocksProxy::start("res-a".into(), "peer-a".into(), tx_a, WebVpnRouter::new())
            .expect("first tunnel");
        let b = SocksProxy::start("res-b".into(), "peer-b".into(), tx_b, WebVpnRouter::new())
            .expect("second tunnel");
        assert_ne!(a.port(), b.port(), "one peer per port");
        for p in [a.port(), b.port()] {
            assert!(
                (PORT_BASE..PORT_BASE + PORT_SPAN).contains(&p),
                "port {p} out of range"
            );
        }
    }

    #[tokio::test]
    async fn the_port_is_free_again_after_a_tunnel_stops() {
        let (tx, _rx) = mpsc::channel::<NetCmd>(1);
        let proxy = SocksProxy::start("res".into(), "peer".into(), tx, WebVpnRouter::new())
            .expect("tunnel");
        let port = proxy.port();
        drop(proxy);

        let mut freed = false;
        for _ in 0..40 {
            if std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok() {
                freed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(freed, "port {port} was never released");
    }
}
