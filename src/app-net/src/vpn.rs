use crate::socks::{connect_through_peer, pump};
use crate::Routers;
use app_core::webvpn::{VpnState, WebVpnRouter};
use app_tun::dns::FakeDns;
use client_core::NetCmd;
use nodeinnet_p2p::P2pMessage;
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot, watch};

/// One for the process: an address an app got before the VPN restarted still has its name.
fn fake_dns() -> Arc<Mutex<FakeDns>> {
    static DNS: OnceLock<Arc<Mutex<FakeDns>>> = OnceLock::new();
    DNS.get_or_init(Default::default).clone()
}

pub struct VpnTunnel {
    peer_id: String,
    online: Arc<AtomicBool>,
    router: Arc<WebVpnRouter>,
    routers: Arc<Routers>,
    stop: Option<oneshot::Sender<()>>,
}

impl VpnTunnel {
    pub fn start(
        fd: OwnedFd,
        mtu: u16,
        peer_id: String,
        resource_id: String,
        online: bool,
        net_tx: mpsc::Sender<NetCmd>,
        routers: Arc<Routers>,
    ) -> std::io::Result<Self> {
        let router = WebVpnRouter::new();
        let online = Arc::new(AtomicBool::new(online));
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<std::io::Result<()>>(1);
        let connections = Connections {
            peer_id: peer_id.clone(),
            resource_id,
            net_tx,
            router: router.clone(),
            online: online.clone(),
            probing: Arc::new(AtomicBool::new(false)),
            closing: watch::channel(false).0,
            live: Arc::new(AtomicUsize::new(0)),
        };
        // The runtime is built and dropped on this thread: in the caller's runtime its drop panics.
        std::thread::Builder::new()
            .name("vpn".into())
            .spawn(move || {
                let started = engine(fd);
                let (runtime, device) = match started {
                    Ok(parts) => parts,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                runtime.block_on(async {
                    let ended = app_tun::run(
                        device,
                        mtu,
                        fake_dns(),
                        |tcp, host, port| connections.open(tcp, host, port),
                        async {
                            let _ = stop_rx.await;
                        },
                    )
                    .await;
                    if let Err(e) = ended {
                        eprintln!("[vpn] ✗ {e}");
                    }
                    connections.close_all().await;
                });
                runtime.shutdown_timeout(Duration::from_secs(1));
            })?;
        ready_rx
            .recv()
            .unwrap_or_else(|_| Err(std::io::Error::other("the VPN thread ended")))?;
        *routers.vpn.write().unwrap() = Some(router.clone());
        Ok(Self {
            peer_id,
            online,
            router,
            routers,
            stop: Some(stop_tx),
        })
    }

    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    pub fn set_online(&self, online: bool) {
        if self.online.swap(online, Ordering::AcqRel) && !online {
            self.router.drain_streams();
        }
    }

    pub fn state(&self) -> VpnState {
        let t = self.router.totals();
        VpnState {
            running: true,
            peer: Some(self.peer_id.clone()),
            online: self.online.load(Ordering::Acquire),
            sockets: t.streams_opened.saturating_sub(t.streams_closed),
            bytes_up: t.bytes_up,
            bytes_down: t.bytes_down,
            generation: 0,
        }
    }
}

impl Drop for VpnTunnel {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let mut slot = self.routers.vpn.write().unwrap();
        if slot.as_ref().is_some_and(|r| Arc::ptr_eq(r, &self.router)) {
            *slot = None;
        }
    }
}

/// ipstack's TCP stream blocks in place when dropped, which only a multi-thread runtime allows.
fn engine(fd: OwnedFd) -> std::io::Result<(tokio::runtime::Runtime, app_tun::TunDevice)> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("vpn")
        .enable_all()
        .build()?;
    let device = {
        let _context = runtime.enter();
        app_tun::TunDevice::new(fd)?
    };
    Ok((runtime, device))
}

struct Connections {
    peer_id: String,
    resource_id: String,
    net_tx: mpsc::Sender<NetCmd>,
    router: Arc<WebVpnRouter>,
    online: Arc<AtomicBool>,
    /// Offline, one connection at a time still probes: ICE may recover without a new handshake.
    probing: Arc<AtomicBool>,
    closing: watch::Sender<bool>,
    live: Arc<AtomicUsize>,
}

impl Connections {
    fn open(&self, tcp: app_tun::TcpStream, host: String, port: u16) {
        let probe = !self.online.load(Ordering::Acquire);
        if probe
            && self
                .probing
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            let live = Live::enter(&self.live);
            tokio::spawn(async move {
                let _live = live;
                refuse(tcp).await;
            });
            return;
        }
        let peer_id = self.peer_id.clone();
        let resource_id = self.resource_id.clone();
        let net_tx = self.net_tx.clone();
        let router = self.router.clone();
        let online = self.online.clone();
        let probing = self.probing.clone();
        let mut closing = self.closing.subscribe();
        let live = Live::enter(&self.live);
        tokio::spawn(async move {
            let _live = live;
            // Not logged when refused: stderr reaches logcat, and the hosts are the user's browsing.
            let relay = async {
                let opened =
                    connect_through_peer(&host, port, &peer_id, &resource_id, &net_tx, &router)
                        .await;
                if probe {
                    online.fetch_or(opened.is_ok(), Ordering::AcqRel);
                    probing.store(false, Ordering::Release);
                }
                match opened {
                    Ok((stream_id, events)) => {
                        pump(
                            tcp,
                            stream_id,
                            events,
                            peer_id.clone(),
                            resource_id.clone(),
                            net_tx.clone(),
                            router.clone(),
                        )
                        .await
                    }
                    Err(_) => refuse(tcp).await,
                }
            };
            tokio::select! {
                _ = relay => {}
                _ = closing.wait_for(|closing| *closing) => {}
            }
        });
    }

    /// Ends every relay while the runtime still runs, so that each stream is dropped on it.
    async fn close_all(&self) {
        let _ = self.closing.send(true);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        // try_send: a waiting send would take every freed slot the network task needs for itself.
        'streams: for stream_id in self.router.drain_streams() {
            let mut close = NetCmd::SendToPeer(
                self.peer_id.clone(),
                P2pMessage::SocksClose {
                    resource_id: self.resource_id.clone(),
                    stream_id,
                },
            );
            loop {
                match self.net_tx.try_send(close) {
                    Ok(()) => break,
                    Err(mpsc::error::TrySendError::Full(back))
                        if tokio::time::Instant::now() < deadline =>
                    {
                        close = back;
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(_) => break 'streams,
                }
            }
        }
        while self.live.load(Ordering::Acquire) > 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

async fn refuse(mut tcp: app_tun::TcpStream) {
    let _ = tokio::time::timeout(Duration::from_secs(2), tcp.shutdown()).await;
}

struct Live(Arc<AtomicUsize>);

impl Live {
    fn enter(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::AcqRel);
        Self(count.clone())
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use etherparse::{PacketBuilder, SlicedPacket, TransportSlice};
    use std::net::Ipv4Addr;
    use std::os::unix::net::UnixDatagram;

    const APP: [u8; 4] = [10, 111, 222, 1];

    #[derive(Clone, Copy, PartialEq)]
    enum Peer {
        Echoes,
        Silent,
        Refuses,
    }

    fn echo_peer(
        routers: Arc<Routers>,
        mut net_rx: mpsc::Receiver<NetCmd>,
        peer: Peer,
    ) -> std::sync::mpsc::Receiver<P2pMessage> {
        let (seen_tx, seen) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Some(NetCmd::SendToPeer(_, msg)) = net_rx.blocking_recv() {
                let _ = seen_tx.send(msg.clone());
                if peer == Peer::Silent {
                    continue;
                }
                let reply = match msg {
                    P2pMessage::SocksConnectRequest {
                        resource_id,
                        stream_id,
                        ..
                    } => P2pMessage::SocksConnectResponse {
                        resource_id,
                        stream_id,
                        is_success: peer == Peer::Echoes,
                        error_msg: None,
                    },
                    data @ P2pMessage::SocksData { .. } => data,
                    _ => continue,
                };
                assert!(routers.route(reply).is_none(), "the VPN claims its replies");
            }
        });
        seen
    }

    struct Phone(UnixDatagram);

    impl Phone {
        fn send(&self, packet: &[u8]) {
            self.0.send(packet).unwrap();
        }

        fn segment(&self, port: u16) -> (u32, String, Vec<u8>) {
            let mut buf = vec![0u8; 2048];
            loop {
                let n = self.0.recv(&mut buf).expect("a packet in time");
                let packet = SlicedPacket::from_ip(&buf[..n]).unwrap();
                if let Some(TransportSlice::Tcp(tcp)) = packet.transport {
                    if tcp.destination_port() != port {
                        continue;
                    }
                    let flags = [
                        (tcp.syn(), 'S'),
                        (tcp.ack(), 'A'),
                        (tcp.fin(), 'F'),
                        (tcp.rst(), 'R'),
                        (tcp.psh(), 'P'),
                    ]
                    .iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, c)| *c)
                    .collect();
                    return (tcp.sequence_number(), flags, tcp.payload().to_vec());
                }
            }
        }

        fn resolve(&self, name: &str) -> Ipv4Addr {
            let mut q = vec![0, 1, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
            for label in name.split('.') {
                q.push(label.len() as u8);
                q.extend_from_slice(label.as_bytes());
            }
            q.extend_from_slice(&[0, 0, 1, 0, 1]);
            let b = PacketBuilder::ipv4(APP, app_tun::dns::RESOLVER.octets(), 64).udp(40000, 53);
            let mut out = Vec::new();
            b.write(&mut out, &q).unwrap();
            self.send(&out);
            let mut buf = vec![0u8; 2048];
            let n = self.0.recv(&mut buf).expect("a DNS answer");
            let answer = SlicedPacket::from_ip(&buf[..n]).unwrap();
            let Some(TransportSlice::Udp(udp)) = answer.transport else {
                panic!("a UDP answer");
            };
            let dns = udp.payload();
            Ipv4Addr::from(<[u8; 4]>::try_from(&dns[dns.len() - 4..]).unwrap())
        }
    }

    fn tcp(port: u16, dst: Ipv4Addr, seq: u32, ack: Option<u32>, payload: &[u8]) -> Vec<u8> {
        let b = PacketBuilder::ipv4(APP, dst.octets(), 64).tcp(port, 80, seq, 65535);
        let b = match ack {
            Some(ack) if payload.is_empty() => b.ack(ack),
            Some(ack) => b.ack(ack).psh(),
            None => b.syn(),
        };
        let mut out = Vec::new();
        b.write(&mut out, payload).unwrap();
        out
    }

    fn fin(port: u16, dst: Ipv4Addr, seq: u32, ack: u32) -> Vec<u8> {
        let b = PacketBuilder::ipv4(APP, dst.octets(), 64)
            .tcp(port, 80, seq, 65535)
            .fin()
            .ack(ack);
        let mut out = Vec::new();
        b.write(&mut out, &[]).unwrap();
        out
    }

    fn rig(
        online: bool,
    ) -> (
        VpnTunnel,
        Phone,
        Arc<Routers>,
        std::sync::mpsc::Receiver<P2pMessage>,
    ) {
        rig_with(online, Peer::Echoes)
    }

    fn rig_with(
        online: bool,
        peer: Peer,
    ) -> (
        VpnTunnel,
        Phone,
        Arc<Routers>,
        std::sync::mpsc::Receiver<P2pMessage>,
    ) {
        let (ours, phone) = UnixDatagram::pair().unwrap();
        phone
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let routers = Routers::new();
        let (net_tx, net_rx) = mpsc::channel(64);
        let seen = echo_peer(routers.clone(), net_rx, peer);
        let vpn = VpnTunnel::start(
            ours.into(),
            1500,
            "peer-1".into(),
            "network-peer-1".into(),
            online,
            net_tx,
            routers.clone(),
        )
        .unwrap();
        (vpn, Phone(phone), routers, seen)
    }

    #[test]
    fn an_app_connection_by_name_reaches_the_peer_and_comes_back() {
        let (vpn, phone, routers, seen) = rig(true);
        let addr = phone.resolve("example.test");

        phone.send(&tcp(40001, addr, 1000, None, &[]));
        let (server_seq, flags, _) = phone.segment(40001);
        assert_eq!(flags, "SA");
        phone.send(&tcp(40001, addr, 1001, Some(server_seq + 1), b"ping"));

        let (echo_seq, echoed) = loop {
            let (seq, _, payload) = phone.segment(40001);
            if !payload.is_empty() {
                break (seq, payload);
            }
        };
        assert_eq!(echoed, b"ping");
        phone.send(&tcp(40001, addr, 1005, Some(echo_seq + 4), &[]));
        assert!(matches!(
            seen.recv_timeout(Duration::from_secs(1)).unwrap(),
            P2pMessage::SocksConnectRequest { ref host, port: 80, ref resource_id, .. }
                if host == "example.test" && resource_id == "network-peer-1"
        ));
        let state = vpn.state();
        assert_eq!((state.sockets, state.bytes_up, state.bytes_down), (1, 4, 4));

        vpn.set_online(false);
        let cut = loop {
            let (_, flags, _) = phone.segment(40001);
            if flags.contains('F') || flags.contains('R') {
                break flags;
            }
        };
        assert!(cut.contains('F'), "the app sees its connection end: {cut}");
        assert_eq!(vpn.state().sockets, 0);

        drop(vpn);
        assert!(
            routers.vpn.read().unwrap().is_none(),
            "the router left the chain"
        );
    }

    #[test]
    fn an_app_closing_its_connection_is_passed_on_to_the_peer_in_seconds() {
        let (_vpn, phone, _routers, seen) = rig(true);
        let addr = phone.resolve("example.test");
        phone.send(&tcp(40003, addr, 3000, None, &[]));
        let (server_seq, _, _) = phone.segment(40003);
        phone.send(&tcp(40003, addr, 3001, Some(server_seq + 1), &[]));
        assert!(matches!(
            seen.recv_timeout(Duration::from_secs(2)).unwrap(),
            P2pMessage::SocksConnectRequest { .. }
        ));

        phone.send(&fin(40003, addr, 3001, server_seq + 1));
        let their_fin = loop {
            let (seq, flags, _) = phone.segment(40003);
            if flags.contains('F') {
                break seq;
            }
        };
        phone.send(&tcp(40003, addr, 3002, Some(their_fin + 1), &[]));

        let closed = std::iter::from_fn(|| seen.recv_timeout(Duration::from_secs(4)).ok())
            .any(|m| matches!(m, P2pMessage::SocksClose { .. }));
        assert!(closed, "the peer hears of it long before the idle limit");
    }

    fn connect(phone: &Phone, port: u16, addr: Ipv4Addr, seq: u32) -> u32 {
        phone.send(&tcp(port, addr, seq, None, &[]));
        let (server_seq, flags, _) = phone.segment(port);
        assert_eq!(flags, "SA");
        phone.send(&tcp(port, addr, seq + 1, Some(server_seq + 1), &[]));
        server_seq
    }

    #[test]
    fn while_the_peer_is_away_one_connection_probes_and_the_others_are_reset() {
        let (_vpn, phone, _routers, seen) = rig_with(false, Peer::Silent);
        let addr = phone.resolve("example.test");

        connect(&phone, 40004, addr, 4000);
        assert!(matches!(
            seen.recv_timeout(Duration::from_secs(2)).unwrap(),
            P2pMessage::SocksConnectRequest { .. }
        ));

        let server_seq = connect(&phone, 40002, addr, 2000);
        // As a kernel would: the first copy may still reach the session being torn down.
        phone
            .0
            .set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        let ended = (0..10).any(|_| {
            phone.send(&tcp(40002, addr, 2001, Some(server_seq + 1), b"GET /"));
            let mut buf = [0u8; 2048];
            std::iter::from_fn(|| phone.0.recv(&mut buf).ok().map(|n| buf[..n].to_vec())).any(|p| {
                matches!(SlicedPacket::from_ip(&p).unwrap().transport,
                    Some(TransportSlice::Tcp(t)) if (t.fin() || t.rst()) && t.destination_port() == 40002)
            })
        });
        assert!(ended, "the app is told its connection ended");
        assert!(
            seen.recv_timeout(Duration::from_millis(300)).is_err(),
            "only the probe was sent to the peer"
        );
    }

    #[test]
    fn a_probe_the_peer_answers_brings_the_vpn_back_online() {
        let (vpn, phone, _routers, seen) = rig(false);
        assert!(!vpn.state().online);
        let addr = phone.resolve("example.test");
        connect(&phone, 40005, addr, 5000);
        assert!(matches!(
            seen.recv_timeout(Duration::from_secs(2)).unwrap(),
            P2pMessage::SocksConnectRequest { .. }
        ));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !vpn.state().online && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            vpn.state().online,
            "the session was there all along, without a new handshake"
        );
    }

    #[test]
    fn a_connection_the_peer_refuses_is_ended_for_an_app_waiting_for_a_reply() {
        let (_vpn, phone, _routers, _seen) = rig_with(true, Peer::Refuses);
        let addr = phone.resolve("no-such-host.example");
        let server_seq = connect(&phone, 40006, addr, 6000);
        phone.send(&tcp(
            40006,
            addr,
            6001,
            Some(server_seq + 1),
            b"ClientHello",
        ));
        let ended = loop {
            let (_, flags, _) = phone.segment(40006);
            if flags.contains('F') || flags.contains('R') {
                break flags;
            }
        };
        assert!(
            ended.contains('F'),
            "the app hears it at once, sending nothing more: {ended}"
        );
    }
}
