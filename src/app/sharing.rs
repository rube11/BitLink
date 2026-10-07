// Chat carries invitations; Quinn carries encrypted streams over our UDP routes.
mod udp;
use std::{
    io,
    net::SocketAddr,
    sync::{Arc, OnceLock, mpsc},
    time::Duration,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use quinn::Endpoint;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinSet,
    time::timeout,
};

const DIRECT_TIMEOUT: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CONNECTIONS: usize = 32;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
static RUNTIME: OnceLock<io::Result<tokio::runtime::Runtime>> = OnceLock::new();

#[derive(Clone)]
pub struct Offer {
    pub port: u16,
    pub global: bool,
    token: [u8; 32],
    id: String,
    certificate: Vec<u8>,
}

impl Offer {
    pub fn parse(body: &str) -> Option<Self> {
        let mut fields = body.splitn(4, ' ');
        let port = fields
            .next()?
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)?;
        let token = STANDARD.decode(fields.next()?).ok()?.try_into().ok()?;
        let id = fields.next()?;
        if id.len() != 24 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let certificate = STANDARD.decode(fields.next()?).ok()?;
        if certificate.is_empty() {
            return None;
        }
        Some(Self {
            port,
            global: false,
            token,
            id: id.into(),
            certificate,
        })
    }
}

pub enum Command {
    Host {
        // None invites global chat; Some invites one person.
        person_id: Option<String>,
        port: u16,
    },
    Join {
        person_id: String,
        offer: Offer,
        port: u16,
    },
    Stop,
}

enum Event {
    Offer(Option<String>, u16, String),
    Status(String),
}

#[derive(Default)]
pub struct Sharing {
    pub command: Option<Command>,
    // Latest invitation from each person, in arrival order.
    pub offers: Vec<(String, Offer)>,
    pub status: String,
    pub peer: Option<String>,
    pub relay: Option<SocketAddr>,
    tasks: JoinSet<()>,
    session: Arc<tokio::sync::Mutex<()>>,
    events: Option<mpsc::Receiver<Event>>,
}

impl Sharing {
    pub fn update(&mut self) -> Option<(Option<String>, u16, String)> {
        while self.tasks.try_join_next().is_some() {}
        if let Some(command) = self.command.take() {
            self.tasks.abort_all();
            self.events = None;
            self.peer = match &command {
                Command::Host { person_id, .. } => person_id.clone(),
                Command::Join { person_id, .. } => Some(person_id.clone()),
                Command::Stop => None,
            };
            if matches!(command, Command::Stop) {
                self.status = "sharing stopped".into();
                return None;
            }
            let runtime = match RUNTIME.get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .build()
            }) {
                Ok(runtime) => runtime,
                Err(error) => {
                    self.status = error.to_string();
                    return None;
                }
            };
            self.status = "starting share…".into();
            let relay = self.relay.unwrap_or_else(|| {
                super::network::DEFAULT_SERVER
                    .parse()
                    .expect("valid default relay")
            });
            let (events, receiver) = mpsc::channel();
            self.events = Some(receiver);
            let session = self.session.clone();
            self.tasks.spawn_on(
                async move {
                    // A replacement waits for the old task to release its listener.
                    let _session = session.lock().await;
                    let status = match run(command, relay, &events).await {
                        Ok(()) => "sharing ended".into(),
                        Err(error) => format!("share failed: {error}"),
                    };
                    let _ = events.send(Event::Status(status));
                },
                runtime.handle(),
            );
        }
        while let Ok(event) = self.events.as_ref()?.try_recv() {
            match event {
                Event::Offer(person_id, port, body) => return Some((person_id, port, body)),
                Event::Status(status) => self.status = status,
            }
        }
        None
    }
}

async fn run(command: Command, relay: SocketAddr, events: &mpsc::Sender<Event>) -> Result<()> {
    let offer = match &command {
        Command::Join { offer, .. } => Some(offer),
        _ => None,
    };
    let (endpoint, socket, certificate) = endpoint(relay, offer)?;
    let session = async {
        match command {
            Command::Host { person_id, port } => {
                timeout(CONNECT_TIMEOUT, socket.registered.notified()).await?;
                let mut token = [0; 32];
                getrandom::fill(&mut token).map_err(|error| io::Error::other(error.to_string()))?;
                let body = format!(
                    "{port} {} {} {}",
                    STANDARD.encode(token),
                    socket.id,
                    STANDARD.encode(certificate)
                );
                events.send(Event::Offer(person_id, port, body))?;
                let _ = events.send(Event::Status(format!("sharing localhost:{port} · /stop")));
                host(&endpoint, port, token).await
            }
            Command::Join { offer, port, .. } => {
                join(&endpoint, &socket, offer, port, events).await
            }
            Command::Stop => unreachable!(),
        }
    };
    let result = tokio::select! {
        result = session => result,
        _ = socket.maintain() => unreachable!(),
    };
    endpoint.close(0u8.into(), b"sharing stopped");
    let _ = timeout(Duration::from_secs(2), endpoint.wait_idle()).await;
    result
}

// Trust only the certificate received inside the encrypted share invitation.
fn endpoint(
    relay: SocketAddr,
    offer: Option<&Offer>,
) -> Result<(Endpoint, Arc<udp::Socket>, Vec<u8>)> {
    let socket = udp::Socket::bind(relay, offer.map(|offer| offer.id.clone()))?;
    let mut transport = quinn::TransportConfig::default();
    transport
        .mtu_discovery_config(None)
        .initial_mtu(1200)
        .max_concurrent_bidi_streams((MAX_CONNECTIONS as u32 + 1).into())
        .max_concurrent_uni_streams(0u8.into())
        .keep_alive_interval(Some(Duration::from_secs(2)));
    let transport = Arc::new(transport);
    let mut config = quinn::EndpointConfig::default();
    config.max_udp_payload_size(1200)?;
    let (server, certificate) = if offer.is_none() {
        let generated = rcgen::generate_simple_self_signed(vec!["club-share".into()])?;
        let certificate = generated.cert.der().to_vec();
        let key = PrivatePkcs8KeyDer::from(generated.signing_key.serialize_der());
        let mut server = quinn::ServerConfig::with_single_cert(
            vec![CertificateDer::from(certificate.clone())],
            key.into(),
        )?;
        server.transport_config(transport.clone());
        (Some(server), certificate)
    } else {
        (None, Vec::new())
    };
    let mut endpoint = Endpoint::new_with_abstract_socket(
        config,
        server,
        socket.clone(),
        Arc::new(quinn::TokioRuntime),
    )?;
    if let Some(offer) = offer {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(offer.certificate.clone()))?;
        let mut client = quinn::ClientConfig::with_root_certificates(Arc::new(roots))?;
        client.transport_config(transport);
        endpoint.set_default_client_config(client);
    }
    Ok((endpoint, socket, certificate))
}

async fn host(endpoint: &Endpoint, port: u16, token: [u8; 32]) -> Result<()> {
    let mut guests = JoinSet::new();
    loop {
        tokio::select! {
            incoming = endpoint.accept(), if guests.len() < MAX_CONNECTIONS => {
                let Some(incoming) = incoming else { break; };
                guests.spawn(async move {
                    let connection = timeout(CONNECT_TIMEOUT, async {
                        let connection = incoming.await?;
                        let (mut send, mut recv) = connection.accept_bi().await?;
                        let mut provided = [0; 32];
                        recv.read_exact(&mut provided).await?;
                        if token.iter().zip(provided)
                            .fold(0, |difference, (a, b)| difference | (a ^ b)) != 0
                        {
                            connection.close(1u8.into(), b"share refused");
                            return Err(io::Error::other("share refused").into());
                        }
                        send.write_all(&[1]).await?;
                        send.finish()?;
                        Result::Ok(connection)
                    }).await??;
                    let mut streams = JoinSet::new();
                    loop {
                        tokio::select! {
                            stream = connection.accept_bi(), if streams.len() < MAX_CONNECTIONS => {
                                let Ok((send, recv)) = stream else { break; };
                                streams.spawn(async move {
                                    // Guests can only reach the port chosen by the host.
                                    let mut socket = TcpStream::connect(("127.0.0.1", port)).await?;
                                    tokio::io::copy_bidirectional(&mut socket, &mut tokio::io::join(recv, send)).await
                                });
                            }
                            _ = streams.join_next(), if !streams.is_empty() => {}
                            _ = connection.closed() => break,
                        }
                    }
                    Result::Ok(())
                });
            }
            _ = guests.join_next(), if !guests.is_empty() => {}
        }
    }
    Ok(())
}

async fn join(
    endpoint: &Endpoint,
    socket: &udp::Socket,
    offer: Offer,
    port: u16,
    events: &mpsc::Sender<Event>,
) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let connection = timeout(CONNECT_TIMEOUT, async {
        socket.introduced.notified().await;
        let address = socket.peers.lock().unwrap().get(&offer.id).unwrap().0;
        let mut connecting = endpoint.connect(address, "club-share")?;
        let connection = match timeout(DIRECT_TIMEOUT, &mut connecting).await {
            Ok(connection) => connection?,
            Err(_) => {
                socket.peers.lock().unwrap().get_mut(&offer.id).unwrap().1 = true;
                connecting.await?
            }
        };
        let (mut send, mut recv) = connection.open_bi().await?;
        send.write_all(&offer.token).await?;
        send.finish()?;
        let mut accepted = [0];
        recv.read_exact(&mut accepted).await?;
        if accepted != [1] {
            return Err(io::Error::other("share refused").into());
        }
        Result::Ok(connection)
    })
    .await??;
    let route = if socket.peers.lock().unwrap().get(&offer.id).unwrap().1 {
        "relay"
    } else {
        "direct"
    };
    let _ = events.send(Event::Status(format!(
        "http://localhost:{port} · {route} · /stop"
    )));
    let mut streams = JoinSet::new();
    loop {
        tokio::select! {
            socket = listener.accept(), if streams.len() < MAX_CONNECTIONS => {
                let (mut socket, _) = socket?;
                let connection = connection.clone();
                streams.spawn(async move {
                    let (send, recv) = connection.open_bi().await.map_err(io::Error::other)?;
                    tokio::io::copy_bidirectional(&mut socket, &mut tokio::io::join(recv, send)).await
                });
            }
            _ = streams.join_next(), if !streams.is_empty() => {}
            _ = connection.closed() => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
