// Encrypted chat and receipts use the relay; retries reuse their ciphertext.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use crate::app::state::{
    App, Delivery, MAX_MESSAGE_BYTES, MAX_MESSAGE_CHARACTERS, MAX_PENDING_MESSAGES, Message,
    OutgoingMessage,
};
use crate::app::{crypto, presence};

pub const DEFAULT_SERVER: &str = "32.189.170.75:47002";

const REGISTER_EVERY: Duration = Duration::from_secs(2);
const RELAY_SILENT_AFTER: Duration = Duration::from_secs(8);
const RESEND_EVERY: Duration = Duration::from_millis(500);
const GIVE_UP_AFTER: Duration = Duration::from_secs(10);
const REMEMBER_RECEIVED_FOR: Duration = Duration::from_secs(60);
const MAX_PACKETS_PER_UPDATE: usize = 256;
const MAX_REMEMBERED_MESSAGES: usize = 4096;
const MAX_TOKEN_BYTES: usize = 64;
const RANDOM_ID_BYTES: usize = 12;

// A message we sent that has not been confirmed yet.
struct PendingMessage {
    outgoing: OutgoingMessage,
    token: String,
    encrypted_payload: String,
    first_sent: Instant,
    next_send: Instant,
}

// A message we already displayed, so a resend of it is not shown twice.
struct ReceivedMessage {
    sender_id: String,
    token: String,
    received_at: Instant,
}

pub struct Network {
    socket: UdpSocket,
    server: SocketAddr,
    id: String,
    pub(super) name: String,
    key: [u8; 32],
    next_register: Instant,
    last_server_reply: Option<Instant>,
    registration_error: Option<&'static str>,
    next_token_number: u64,
    pending: Vec<PendingMessage>,
    received: Vec<ReceivedMessage>,
}

impl Network {
    // chosen_name is None when the user did not pass --name.
    pub fn connect(
        chosen_name: Option<String>,
        server: &str,
        key: [u8; 32],
    ) -> io::Result<Network> {
        let server: SocketAddr = match server.parse() {
            Ok(address) => address,
            Err(error) => return Err(io::Error::other(error)),
        };

        let id = match random_id() {
            Ok(id) => id,
            Err(error) => return Err(error),
        };
        let name = match chosen_name {
            Some(name) => name,
            None => format!("Member {}", &id[..6]),
        };
        if !presence::is_valid_name(&name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Use a name of 1–40 characters without control characters.",
            ));
        }

        // Port 0 lets the operating system pick any free port.
        let socket = match UdpSocket::bind("0.0.0.0:0") {
            Ok(socket) => socket,
            Err(error) => return Err(error),
        };
        // Non-blocking means recv_from returns immediately when nothing arrived,
        // so the event loop keeps drawing and reading the keyboard.
        match socket.set_nonblocking(true) {
            Ok(()) => {}
            Err(error) => return Err(error),
        }

        return Ok(Network {
            socket: socket,
            server: server,
            id: id,
            name: name,
            key: key,
            next_register: Instant::now(),
            last_server_reply: None,
            registration_error: None,
            next_token_number: 0,
            pending: Vec::new(),
            received: Vec::new(),
        });
    }

    // Called once per event loop iteration.
    pub fn update(&mut self, app: &mut App, now: Instant) {
        if now >= self.next_register {
            self.send(&format!("REGISTER {} {}", self.id, self.name));
            self.next_register = now + REGISTER_EVERY;
        }
        self.receive_packets(app, now);
        let connected = self
            .last_server_reply
            .is_some_and(|reply| now.duration_since(reply) < RELAY_SILENT_AFTER);
        app.network_status = String::from(match self.registration_error {
            Some(error) => error,
            None if connected => "Relay connected",
            None => "Relay unavailable · retrying",
        });
        presence::remove_missing_people(app, now);
        self.sync_peers(app);
        self.queue_outgoing_messages(app, now);
        self.resend_pending_messages(app, now);
        self.received
            .retain(|message| now.duration_since(message.received_at) < REMEMBER_RECEIVED_FOR);
    }

    pub fn goodbye(&self) {
        self.send(&format!("GOODBYE {}", self.id));
    }

    fn receive_packets(&mut self, app: &mut App, now: Instant) {
        let mut buffer = [0_u8; 4096];
        for _packet_number in 0..MAX_PACKETS_PER_UPDATE {
            let (count, source) = match self.socket.recv_from(&mut buffer) {
                Ok(received) => received,
                Err(error) => {
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    // WouldBlock means nothing else is waiting right now.
                    break;
                }
            };
            // A packet that fills the whole buffer may have been cut short.
            if count == buffer.len() {
                continue;
            }
            let text = match std::str::from_utf8(&buffer[..count]) {
                Ok(text) => text,
                Err(_) => continue,
            };
            if source == self.server {
                self.handle_packet(app, text, now);
            }
        }
    }

    fn handle_packet(&mut self, app: &mut App, text: &str, now: Instant) {
        if text.starts_with("REGISTERED ") {
            self.last_server_reply = Some(now);
            self.registration_error = None;
            return;
        }
        if text == "ERROR address-in-use" || text == "ERROR id-in-use" {
            self.last_server_reply = None;
            self.registration_error = Some(if text == "ERROR address-in-use" {
                "Reconnect blocked · retrying"
            } else {
                "Session ID in use · retrying"
            });
            return;
        }
        if text.starts_with("bit-to-byte/1\n") {
            presence::receive_packet(app, text, &self.id, now);
            self.sync_peers(app);
            return;
        }
        // Only encrypted chat packets are accepted. Never fall back to plaintext.
        let mut words = text.splitn(4, ' ');
        let (sender_id, encrypted) = match (words.next(), words.next(), words.next(), words.next())
        {
            (Some("FROM"), Some(sender), Some("ENC1"), Some(encrypted)) => (sender, encrypted),
            _ => return,
        };
        let plaintext = match crypto::decrypt(&self.key, sender_id, &self.id, encrypted) {
            Ok(plaintext) => plaintext,
            Err(_) => return,
        };
        let mut fields = plaintext.splitn(3, ' ');
        let (token, kind) = match (fields.next(), fields.next()) {
            (Some(token), Some(kind)) if !token.is_empty() && token.len() <= MAX_TOKEN_BYTES => {
                (token, kind)
            }
            _ => return,
        };
        let body = fields.next().unwrap_or("");

        match (kind, body) {
            ("RECEIPT", "") => {
                if let Some(index) = self.pending.iter().position(|message| {
                    message.outgoing.person_id == sender_id && message.token == token
                }) {
                    let confirmed = self.pending.remove(index);
                    mark_delivery(app, &confirmed.outgoing, Delivery::Delivered);
                }
            }
            ("CHAT", _) => {
                self.handle_chat(app, sender_id, token, body, now);
            }
            _ => {}
        }
    }

    fn handle_chat(
        &mut self,
        app: &mut App,
        sender_id: &str,
        token: &str,
        text: &str,
        now: Instant,
    ) {
        if text.is_empty()
            || text.len() > MAX_MESSAGE_BYTES
            || text.chars().count() > MAX_MESSAGE_CHARACTERS
            || text.chars().any(char::is_control)
        {
            return;
        }
        // Only accept messages from people in our list.
        let person = match app.find_person(sender_id) {
            Some(person) => person,
            None => return,
        };

        let already_received = self
            .received
            .iter()
            .any(|message| message.sender_id == sender_id && message.token == token);
        if !already_received {
            if self.received.len() >= MAX_REMEMBERED_MESSAGES {
                return;
            }
            person.messages.push(Message::received(text));
            self.received.push(ReceivedMessage {
                sender_id: String::from(sender_id),
                token: String::from(token),
                received_at: now,
            });
        }
        // Send a receipt even for a repeat, because the first one may have been lost.
        if let Ok(packet) = self.encrypted_payload(sender_id, &format!("{} RECEIPT", token)) {
            self.send(&format!("RELAY {} {}", sender_id, packet));
        }
    }

    // Move messages the user just sent from the app outbox into our pending list.
    fn queue_outgoing_messages(&mut self, app: &mut App, now: Instant) {
        for outgoing in std::mem::take(&mut app.outbox) {
            if self.pending.len() >= MAX_PENDING_MESSAGES {
                mark_delivery(app, &outgoing, Delivery::Unconfirmed);
                continue;
            }
            self.next_token_number += 1;
            let token = format!("{}-{}", self.id, self.next_token_number);
            let payload = format!("{} CHAT {}", token, outgoing.text);
            let packet = match self.encrypted_payload(&outgoing.person_id, &payload) {
                Ok(packet) => packet,
                Err(_) => {
                    mark_delivery(app, &outgoing, Delivery::Unconfirmed);
                    continue;
                }
            };
            self.pending.push(PendingMessage {
                outgoing: outgoing,
                token: token,
                encrypted_payload: packet,
                first_sent: now,
                next_send: now,
            });
        }
    }

    fn resend_pending_messages(&mut self, app: &mut App, now: Instant) {
        let mut index = 0;
        while index < self.pending.len() {
            let waited = now.duration_since(self.pending[index].first_sent);
            if waited >= GIVE_UP_AFTER {
                let given_up = self.pending.remove(index);
                mark_delivery(app, &given_up.outgoing, Delivery::Unconfirmed);
                continue;
            }
            if now >= self.pending[index].next_send {
                // Retry the exact encrypted packet, including its original nonce.
                let pending = &self.pending[index];
                self.send(&format!(
                    "RELAY {} {}",
                    pending.outgoing.person_id, pending.encrypted_payload
                ));
                self.pending[index].next_send = now + RESEND_EVERY;
            }
            index += 1;
        }
    }

    fn encrypted_payload(&self, recipient: &str, payload: &str) -> io::Result<String> {
        let encrypted = match crypto::encrypt(&self.key, &self.id, recipient, payload) {
            Ok(encrypted) => encrypted,
            Err(error) => return Err(error),
        };
        // ENC1 and its space also count toward the deployed relay's 2100-byte limit.
        if encrypted.len() + 5 > 2100 {
            return Err(io::Error::other(
                "Encrypted message is too large for the relay.",
            ));
        }
        return Ok(format!("ENC1 {}", encrypted));
    }

    fn sync_peers(&mut self, app: &App) {
        self.pending.retain(|message| {
            app.people
                .iter()
                .any(|person| person.id == message.outgoing.person_id)
        });
    }

    fn send(&self, packet: &str) {
        // A failed send is not fatal: registration and messages are repeated later.
        let _result = self.socket.send_to(packet.as_bytes(), self.server);
    }
}

// Update the delivery label on the message the user sent.
fn mark_delivery(app: &mut App, outgoing: &OutgoingMessage, delivery: Delivery) {
    let person = match app.find_person(&outgoing.person_id) {
        Some(person) => person,
        None => return,
    };
    if let Some(message) = person.messages.get_mut(outgoing.message_index) {
        message.delivery = Some(delivery);
    }
}

// A fresh ID for this run, written as 24 hexadecimal characters.
fn random_id() -> io::Result<String> {
    let mut bytes = [0_u8; RANDOM_ID_BYTES];
    match getrandom::fill(&mut bytes) {
        Ok(()) => {}
        Err(error) => return Err(io::Error::other(error.to_string())),
    }
    let mut id = String::new();
    for byte in bytes {
        id.push_str(&format!("{:02x}", byte));
    }
    return Ok(id);
}

#[cfg(test)]
mod tests;
