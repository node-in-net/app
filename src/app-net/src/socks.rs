use app_core::webvpn::{StreamEvent, WebVpnRouter};
use client_core::NetCmd;
use nodeinnet_p2p::P2pMessage;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

const PORT_BASE: u16 = 54145;
const PORT_SPAN: u16 = 100;

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

    let stream_id = Uuid::new_v4();
    let mut events = router.register_stream(stream_id);
    let _ = net_tx
        .send(NetCmd::SendToPeer(
            peer_id.clone(),
            P2pMessage::SocksConnectRequest {
                resource_id: resource_id.clone(),
                stream_id,
                host: host.clone(),
                port,
            },
        ))
        .await;

    let outcome = tokio::time::timeout(Duration::from_secs(10), events.recv()).await;
    let connected = match &outcome {
        Ok(Some(StreamEvent::Connected { is_success, .. })) => *is_success,
        _ => false,
    };
    if !connected {
        match outcome {
            Ok(Some(StreamEvent::Connected { error_msg, .. })) => eprintln!(
                "[net] ✗ {peer_id} refused {host}:{port}: {}",
                error_msg.unwrap_or_else(|| "no reason given".into())
            ),
            Ok(_) => eprintln!("[net] ✗ {peer_id} closed the stream to {host}:{port}"),
            Err(_) => eprintln!("[net] ✗ {peer_id} did not answer for {host}:{port}"),
        }
        stream
            .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await?;
        router.close_stream(stream_id);
        return Ok(());
    }
    stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;

    let (mut rd, mut wr) = stream.into_split();
    tokio::spawn(async move {
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
        let _ = wr.shutdown().await;
    });

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
            peer_id,
            P2pMessage::SocksClose {
                resource_id,
                stream_id,
            },
        ))
        .await;
    router.close_stream(stream_id);
    Ok(())
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
