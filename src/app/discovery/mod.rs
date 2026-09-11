// Every app joins the same UDP multicast group. There is no central server.
mod socket;

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::app::presence;
use crate::app::state::App;

const DISCOVERY_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 99);
const DISCOVERY_PORT: u16 = 47001;
const HELLO_INTERVAL: Duration = Duration::from_secs(2);
const PACKET_BUFFER_SIZE: usize = 512;
const MAX_PACKETS_PER_UPDATE: usize = 64;

pub struct Discovery {
    socket: UdpSocket,
    local_sender: UdpSocket,
    id: String,
    name: String,
    next_hello: Instant,
}

impl Discovery {
    pub fn start(name: &str) -> io::Result<Discovery> {
        if !presence::is_valid_name(name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Use a name of 1–40 characters without control characters.",
            ));
        }
        let socket = match socket::open_discovery_socket() {
            Ok(socket) => socket,
            Err(error) => return Err(error),
        };
        let local_sender = match socket::open_local_sender() {
            Ok(local_sender) => local_sender,
            Err(error) => return Err(error),
        };
        let started = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(started) => started,
            Err(error) => return Err(io::Error::other(error)),
        };

        // Identify this launch, so two people with the same name stay separate.
        let id = format!("{}-{}", started.as_nanos(), std::process::id());
        return Ok(Discovery {
            socket: socket,
            local_sender: local_sender,
            id: id,
            name: String::from(name),
            next_hello: Instant::now(),
        });
    }

    pub fn update(&mut self, app: &mut App) -> io::Result<()> {
        let now = Instant::now();
        if now >= self.next_hello {
            match self.send_packet("hello") {
                Ok(()) => {}
                Err(error) => return Err(error),
            }
            self.next_hello = now + HELLO_INTERVAL;
        }

        match self.receive_packets(app, now) {
            Ok(()) => {}
            Err(error) => return Err(error),
        }
        presence::mark_missing_people_offline(app, now);
        return Ok(());
    }

    pub fn goodbye(&self) -> io::Result<()> {
        return self.send_packet("goodbye");
    }

    fn receive_packets(&self, app: &mut App, now: Instant) -> io::Result<()> {
        // Limit work per frame so incoming traffic cannot block the keyboard.
        let mut buffer = [0_u8; PACKET_BUFFER_SIZE];
        for _attempt in 0..MAX_PACKETS_PER_UPDATE {
            match self.socket.recv_from(&mut buffer) {
                Ok((count, _sender)) => {
                    // A full buffer may mean the packet was cut short.
                    // Our valid discovery packets are smaller than the buffer.
                    if count == buffer.len() {
                        continue;
                    }
                    let packet = &buffer[..count];
                    presence::receive_packet(app, packet, &self.id, now);
                }
                Err(error) => {
                    if error.kind() == io::ErrorKind::WouldBlock {
                        break;
                    }
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(error);
                }
            }
        }
        return Ok(());
    }

    fn send_packet(&self, message_type: &str) -> io::Result<()> {
        let packet = format!(
            "bit-to-byte/1\n{}\n{}\n{}",
            message_type, self.id, self.name
        );
        let destination = SocketAddrV4::new(DISCOVERY_GROUP, DISCOVERY_PORT);
        // Send locally as well: some firewalls filter even our own Wi-Fi traffic.
        for sender in [&self.local_sender, &self.socket] {
            match sender.send_to(packet.as_bytes(), destination) {
                Ok(_) => {}
                Err(error) => {
                    // A missed packet is okay. The next hello retries.
                    if error.kind() == io::ErrorKind::WouldBlock
                        || error.kind() == io::ErrorKind::Interrupted
                    {
                        continue;
                    }
                    return Err(error);
                }
            }
        }
        return Ok(());
    }
}
