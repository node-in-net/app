#[cfg(unix)]
mod device;
pub mod dns;

#[cfg(unix)]
pub use device::TunDevice;

use dns::FakeDns;
use ipstack::{
    IpStack, IpStackConfig, IpStackStream, IpStackTcpStream, IpStackUdpStream, TcpConfig,
    TcpOptions,
};
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

const TCP_IDLE: Duration = Duration::from_secs(600);
const DNS_IDLE: Duration = Duration::from_secs(10);
const SHUTDOWN_REPOLL: Duration = Duration::from_millis(50);
const REPOLL: Duration = Duration::from_secs(2);

/// ipstack wakes no pending FIN, nor reads or writes after close; re-polls reset its idle timer.
pub struct TcpStream {
    inner: IpStackTcpStream,
    active: tokio::time::Instant,
    read_tick: Option<Pin<Box<tokio::time::Sleep>>>,
    write_tick: Option<Pin<Box<tokio::time::Sleep>>>,
    shutdown_tick: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl TcpStream {
    fn new(inner: IpStackTcpStream) -> Self {
        Self {
            inner,
            active: tokio::time::Instant::now(),
            read_tick: None,
            write_tick: None,
            shutdown_tick: None,
        }
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.inner.peer_addr()
    }
}

fn arm(tick: &mut Option<Pin<Box<tokio::time::Sleep>>>, every: Duration, cx: &mut Context<'_>) {
    let tick = tick.get_or_insert_with(|| Box::pin(tokio::time::sleep(every)));
    if tick.as_mut().poll(cx).is_ready() {
        tick.as_mut().reset(tokio::time::Instant::now() + every);
        let _ = tick.as_mut().poll(cx);
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;
        let before = buf.filled().len();
        let polled = Pin::new(&mut this.inner).poll_read(cx, buf);
        match polled {
            Poll::Ready(Ok(())) if buf.filled().len() > before => {
                this.active = tokio::time::Instant::now();
            }
            Poll::Pending if this.active.elapsed() >= TCP_IDLE => {
                return Poll::Ready(Err(io::ErrorKind::TimedOut.into()));
            }
            Poll::Pending => arm(&mut this.read_tick, REPOLL, cx),
            _ => {}
        }
        polled
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = &mut *self;
        let polled = Pin::new(&mut this.inner).poll_write(cx, buf);
        match polled {
            Poll::Ready(Ok(n)) if n > 0 => this.active = tokio::time::Instant::now(),
            Poll::Pending if this.active.elapsed() >= TCP_IDLE => {
                return Poll::Ready(Err(io::ErrorKind::TimedOut.into()));
            }
            Poll::Pending => arm(&mut this.write_tick, REPOLL, cx),
            _ => {}
        }
        polled
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;
        let polled = Pin::new(&mut this.inner).poll_shutdown(cx);
        if polled.is_pending() {
            arm(&mut this.shutdown_tick, SHUTDOWN_REPOLL, cx);
        }
        polled
    }
}

/// ipstack's TCP streams block in place on drop: run this and drop them on a multi-thread runtime.
pub async fn run<D, C>(
    device: D,
    mtu: u16,
    dns: Arc<Mutex<FakeDns>>,
    connect: C,
    shutdown: impl Future<Output = ()>,
) -> std::io::Result<()>
where
    D: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    C: Fn(TcpStream, String, u16),
{
    let mut config = IpStackConfig::default();
    config
        .mtu(mtu)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?
        .udp_timeout(DNS_IDLE);
    let mut tcp = TcpConfig::default();
    tcp.timeout = 2 * TCP_IDLE;
    // Without it the app's kernel falls back to 536-byte segments.
    tcp.options = Some(vec![TcpOptions::MaximumSegmentSize(mtu - 40)]);
    config.with_tcp_config(tcp);
    let mut stack = IpStack::new(config, device);

    tokio::pin!(shutdown);
    loop {
        let stream = tokio::select! {
            _ = &mut shutdown => return Ok(()),
            accepted = stack.accept() => match accepted {
                Ok(stream) => stream,
                Err(_) => return Err(std::io::Error::other("the TUN device stopped")),
            },
        };
        match stream {
            IpStackStream::Tcp(inner) => {
                let dst = inner.peer_addr();
                if let Some(host) = host_of(&dns, dst.ip()) {
                    connect(TcpStream::new(inner), host, dst.port());
                }
            }
            IpStackStream::Udp(udp) if udp.peer_addr().port() == 53 => {
                tokio::spawn(answer_dns(udp, dns.clone()));
            }
            _ => {}
        }
    }
}

fn host_of(dns: &Mutex<FakeDns>, ip: IpAddr) -> Option<String> {
    match ip {
        IpAddr::V4(v4) if FakeDns::is_fake(v4) => {
            dns.lock().unwrap().name_of(v4).map(str::to_string)
        }
        ip => Some(ip.to_string()),
    }
}

async fn answer_dns(mut udp: IpStackUdpStream, dns: Arc<Mutex<FakeDns>>) {
    let mut query = vec![0u8; 1500];
    loop {
        let n = match udp.read(&mut query).await {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let reply = dns.lock().unwrap().answer(&query[..n]);
        if let Some(reply) = reply {
            if udp.write_all(&reply).await.is_err() {
                return;
            }
        }
    }
}
