// The same UDP socket registers, punches NAT mappings, and carries QUIC.
// Relay envelopes preserve the peer's address so Quinn still sees one connection.
use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    io::{self, IoSliceMut},
    net::{SocketAddr, UdpSocket},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, ready},
    time::Duration,
};

use quinn::{
    AsyncUdpSocket, UdpPoller,
    udp::{RecvMeta, Transmit},
};
use tokio::{io::ReadBuf, sync::Notify};

#[derive(Debug)]
pub(super) struct Socket {
    inner: Arc<tokio::net::UdpSocket>,
    relay: SocketAddr,
    pub id: String,
    target: Option<String>,
    pub peers: Mutex<BTreeMap<String, (SocketAddr, bool)>>,
    pub registered: Notify,
    pub introduced: Notify,
}

impl Socket {
    pub fn bind(relay: SocketAddr, target: Option<String>) -> io::Result<Arc<Self>> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        Ok(Arc::new(Self {
            inner: Arc::new(tokio::net::UdpSocket::from_std(socket)?),
            relay,
            id: super::super::network::random_id()?,
            target,
            peers: Mutex::new(BTreeMap::new()),
            registered: Notify::new(),
            introduced: Notify::new(),
        }))
    }

    pub async fn maintain(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let _ = self.inner.try_send_to(
                format!("QUIC REGISTER {} Sharing", self.id).as_bytes(),
                self.relay,
            );
            if let Some(target) = &self.target {
                let _ = self
                    .inner
                    .try_send_to(format!("QUIC DISCOVER {target}").as_bytes(), self.relay);
            }
            for (address, _) in self.peers.lock().unwrap().values() {
                let _ = self.inner.try_send_to(b"QUIC PUNCH", *address);
            }
        }
    }
}

impl AsyncUdpSocket for Socket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(Poller {
            socket: self.inner.clone(),
            writable: None,
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        let peers = self.peers.lock().unwrap();
        let Some((id, (_, relayed))) = peers
            .iter()
            .find(|(_, (address, _))| *address == transmit.destination)
        else {
            return Ok(());
        };
        if !relayed {
            return self
                .inner
                .try_send_to(transmit.contents, transmit.destination)
                .map(|_| ());
        }
        let mut packet = format!("QUIC DATA {id} ").into_bytes();
        packet.extend_from_slice(transmit.contents);
        self.inner.try_send_to(&packet, self.relay).map(|_| ())
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        // Leave room for the relay header while receiving a QUIC datagram.
        let mut buffer = [0; 4096];
        for _ in 0..32 {
            let mut received = ReadBuf::new(&mut buffer);
            let source = ready!(self.inner.poll_recv_from(cx, &mut received))?;
            let packet = received.filled();
            let (data, address) = if source == self.relay {
                if let Some(body) = packet.strip_prefix(b"QUIC FROM ") {
                    let Some(split) = body.iter().position(|byte| *byte == b' ') else {
                        continue;
                    };
                    let Ok(id) = std::str::from_utf8(&body[..split]) else {
                        continue;
                    };
                    let mut peers = self.peers.lock().unwrap();
                    let Some((address, relayed)) = peers.get_mut(id) else {
                        continue;
                    };
                    *relayed = true;
                    (&body[split + 1..], *address)
                } else {
                    let Ok(text) = std::str::from_utf8(packet) else {
                        continue;
                    };
                    if text.starts_with("QUIC REGISTERED ") {
                        self.registered.notify_one();
                    }
                    if let Some(body) = text.strip_prefix("QUIC PEER ") {
                        let mut fields = body.splitn(3, ' ');
                        if let (Some(id), Some(address)) = (fields.next(), fields.next())
                            && self.target.as_deref().is_none_or(|target| target == id)
                            && let Ok(address) = address.parse::<SocketAddr>()
                        {
                            let mut peers = self.peers.lock().unwrap();
                            if peers.len() < 128 || peers.contains_key(id) {
                                peers.entry(id.into()).or_insert((address, false));
                                self.introduced.notify_one();
                            }
                        }
                    }
                    continue;
                }
            } else {
                let peers = self.peers.lock().unwrap();
                if !peers.values().any(|(address, _)| *address == source) || packet == b"QUIC PUNCH"
                {
                    continue;
                }
                (packet, source)
            };
            if data.len() > bufs[0].len() {
                continue;
            }
            bufs[0][..data.len()].copy_from_slice(data);
            meta[0] = RecvMeta {
                addr: address,
                len: data.len(),
                stride: data.len(),
                ecn: None,
                dst_ip: None,
            };
            return Poll::Ready(Ok(1));
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = self
            .inner
            .try_send_to(format!("QUIC GOODBYE {}", self.id).as_bytes(), self.relay);
    }
}

// Each connection needs its own readiness future; poll_send_ready shares one waker.
struct Poller {
    socket: Arc<tokio::net::UdpSocket>,
    writable: Option<Pin<Box<dyn Future<Output = io::Result<()>> + Send + Sync>>>,
}

impl fmt::Debug for Poller {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UDP write readiness")
    }
}

impl UdpPoller for Poller {
    fn poll_writable(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let poller = self.get_mut();
        if poller.writable.is_none() {
            let socket = poller.socket.clone();
            poller.writable = Some(Box::pin(async move { socket.writable().await }));
        }
        let result = poller.writable.as_mut().unwrap().as_mut().poll(cx);
        if result.is_ready() {
            poller.writable = None;
        }
        result
    }
}
