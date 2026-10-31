#![cfg(unix)]

use app_tun::dns::{FakeDns, RESOLVER};
use etherparse::{PacketBuilder, SlicedPacket, TcpOptionElement, TransportSlice};
use std::net::Ipv4Addr;
use std::os::unix::net::UnixDatagram;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const APP: [u8; 4] = [10, 111, 222, 1];
const MTU: u16 = 1500;

struct Rig {
    phone: UnixDatagram,
    connects: mpsc::Receiver<(String, u16)>,
    stop: tokio::sync::oneshot::Sender<()>,
    engine: tokio::task::JoinHandle<std::io::Result<()>>,
}

fn rig() -> Rig {
    let (ours, phone) = UnixDatagram::pair().unwrap();
    phone
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let device = app_tun::TunDevice::new(ours.into()).unwrap();
    let (tx, connects) = mpsc::channel();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let dns = Arc::new(Mutex::new(FakeDns::new()));
    let engine = tokio::spawn(app_tun::run(
        device,
        MTU,
        dns,
        move |tcp, host, port| {
            let _ = tx.send((host, port));
            drop(tcp);
        },
        async {
            let _ = stopped.await;
        },
    ));
    Rig {
        phone,
        connects,
        stop,
        engine,
    }
}

impl Rig {
    fn send(&self, packet: &[u8]) {
        self.phone.send(packet).unwrap();
    }

    fn recv(&self) -> Vec<u8> {
        let mut buf = vec![0u8; 2048];
        let n = self.phone.recv(&mut buf).expect("a packet back");
        buf.truncate(n);
        buf
    }

    async fn stop(self) {
        let _ = self.stop.send(());
        let ended = tokio::time::timeout(Duration::from_secs(3), self.engine).await;
        assert!(
            matches!(ended, Ok(Ok(Ok(())))),
            "a stop ends the stack cleanly"
        );
    }
}

fn udp(src_port: u16, dst: Ipv4Addr, dst_port: u16, payload: &[u8]) -> Vec<u8> {
    let b = PacketBuilder::ipv4(APP, dst.octets(), 64).udp(src_port, dst_port);
    let mut out = Vec::with_capacity(b.size(payload.len()));
    b.write(&mut out, payload).unwrap();
    out
}

fn syn(src_port: u16, dst: Ipv4Addr, dst_port: u16) -> Vec<u8> {
    let b = PacketBuilder::ipv4(APP, dst.octets(), 64)
        .tcp(src_port, dst_port, 1000, 65535)
        .syn();
    let mut out = Vec::with_capacity(b.size(0));
    b.write(&mut out, &[]).unwrap();
    out
}

fn a_query(id: u16, name: &str) -> Vec<u8> {
    let mut q = id.to_be_bytes().to_vec();
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 1, 0, 1]);
    q
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connection_to_an_answered_address_carries_the_name() {
    let rig = rig();
    rig.send(&udp(40000, RESOLVER, 53, &a_query(0x77, "example.com")));
    let reply = rig.recv();
    let packet = SlicedPacket::from_ip(&reply).unwrap();
    let Some(TransportSlice::Udp(answer)) = packet.transport else {
        panic!("a UDP reply");
    };
    assert_eq!(
        (answer.source_port(), answer.destination_port()),
        (53, 40000)
    );
    let dns = answer.payload();
    assert_eq!(dns[..2], [0, 0x77]);
    let addr = Ipv4Addr::from(<[u8; 4]>::try_from(&dns[dns.len() - 4..]).unwrap());
    assert!(FakeDns::is_fake(addr));

    rig.send(&syn(40001, addr, 443));
    let handshake = rig.recv();
    let packet = SlicedPacket::from_ip(&handshake).unwrap();
    let Some(TransportSlice::Tcp(syn_ack)) = packet.transport else {
        panic!("a TCP reply");
    };
    assert!(syn_ack.syn() && syn_ack.ack());
    assert!(
        syn_ack
            .options_iterator()
            .any(|o| matches!(o, Ok(TcpOptionElement::MaximumSegmentSize(1460)))),
        "the MSS follows the MTU"
    );
    assert_eq!(
        rig.connects.recv_timeout(Duration::from_secs(3)).unwrap(),
        ("example.com".to_string(), 443)
    );
    rig.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pool_address_without_a_name_is_never_passed_on() {
    let rig = rig();
    rig.send(&syn(40002, Ipv4Addr::new(198, 18, 0, 77), 443));
    rig.send(&syn(40003, RESOLVER, 853));
    assert!(rig
        .connects
        .recv_timeout(Duration::from_millis(500))
        .is_err());
    rig.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_address_outside_the_pool_is_passed_on_as_text() {
    let rig = rig();
    rig.send(&syn(40004, Ipv4Addr::new(93, 184, 216, 34), 80));
    assert_eq!(
        rig.connects.recv_timeout(Duration::from_secs(3)).unwrap(),
        ("93.184.216.34".to_string(), 80)
    );
    rig.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn udp_other_than_dns_gets_nothing_back() {
    let rig = rig();
    rig.phone
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    rig.send(&udp(40005, Ipv4Addr::new(8, 8, 8, 8), 443, b"quic"));
    let mut buf = [0u8; 64];
    assert!(rig.phone.recv(&mut buf).is_err(), "no datagram leaves");
    rig.stop().await;
}
