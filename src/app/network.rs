// Talk to the relay server over one UDP socket.
//
// Every two seconds we send:    REGISTER <our id> <our name>
// The relay answers with:       REGISTERED <our public address>
// followed by a hello packet for each other named person (see presence.rs).
//
// To chat we send:              RELAY <person id> <token> CHAT <text>
// The relay forwards it as:     FROM <sender id> <token> CHAT <text>
// The receiving app answers:    RELAY <sender id> <token> RECEIPT
// which arrives back as:        FROM <person id> <token> RECEIPT
//
// UDP can lose packets, so we resend a message until its receipt arrives or
// we give up. The token lets both sides recognise repeats of the same message.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use crate::app::presence;
use crate::app::state::{
    App, Delivery, MAX_MESSAGE_CHARACTERS, MAX_PENDING_MESSAGES, Message, OutgoingMessage,
};

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
    name: String,
    next_register: Instant,
    last_server_reply: Option<Instant>,
    next_token_number: u64,
    pending: Vec<PendingMessage>,
    received: Vec<ReceivedMessage>,
}

impl Network {
    // chosen_name is None when the user did not pass --name.
    pub fn connect(chosen_name: Option<String>) -> io::Result<Network> {
        return Network::connect_to(chosen_name, DEFAULT_SERVER);
    }

    fn connect_to(chosen_name: Option<String>, server: &str) -> io::Result<Network> {
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
            next_register: Instant::now(),
            last_server_reply: None,
            next_token_number: 0,
            pending: Vec::new(),
            received: Vec::new(),
        });
    }

    pub fn name(&self) -> &str {
        return &self.name;
    }

    // Called once per event loop iteration.
    pub fn update(&mut self, app: &mut App) {
        let now = Instant::now();
        self.register_if_due(now);
        self.receive_packets(app, now);
        self.update_status(app, now);
        presence::mark_missing_people_offline(app, now);
        self.queue_outgoing_messages(app, now);
        self.resend_pending_messages(app, now);
        self.forget_old_received_messages(now);
    }

    pub fn goodbye(&self) {
        self.send(&format!("GOODBYE {}", self.id));
    }

    fn register_if_due(&mut self, now: Instant) {
        if now < self.next_register {
            return;
        }
        self.send(&format!("REGISTER {} {}", self.id, self.name));
        self.next_register = now + REGISTER_EVERY;
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
            // Only trust packets from the relay we registered with.
            if source != self.server {
                continue;
            }
            // A packet that fills the whole buffer may have been cut short.
            if count == buffer.len() {
                continue;
            }
            let text = match std::str::from_utf8(&buffer[..count]) {
                Ok(text) => text,
                Err(_) => continue,
            };
            self.handle_packet(app, text, now);
        }
    }

    fn handle_packet(&mut self, app: &mut App, text: &str, now: Instant) {
        if text.starts_with("REGISTERED ") {
            self.last_server_reply = Some(now);
            return;
        }
        if presence::is_presence_packet(text) {
            presence::receive_packet(app, text, &self.id, now);
            return;
        }

        // FROM <sender id> <token> CHAT <text>   or   FROM <sender id> <token> RECEIPT
        let mut words = Vec::new();
        for word in text.splitn(5, ' ') {
            words.push(word);
        }
        if words.len() < 4 || words[0] != "FROM" {
            return;
        }
        let sender_id = words[1];
        let token = words[2];
        let kind = words[3];
        let mut body = "";
        if words.len() == 5 {
            body = words[4];
        }
        if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
            return;
        }

        if kind == "RECEIPT" && body.is_empty() {
            self.handle_receipt(app, sender_id, token);
        } else if kind == "CHAT" {
            self.handle_chat(app, sender_id, token, body, now);
        }
    }

    fn handle_receipt(&mut self, app: &mut App, sender_id: &str, token: &str) {
        let mut index = 0;
        while index < self.pending.len() {
            let matches = self.pending[index].outgoing.person_id == sender_id
                && self.pending[index].token == token;
            if matches {
                let confirmed = self.pending.remove(index);
                mark_delivery(app, &confirmed.outgoing, Delivery::Delivered);
                return;
            }
            index += 1;
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
        if text.is_empty() || text.chars().count() > MAX_MESSAGE_CHARACTERS {
            return;
        }
        for character in text.chars() {
            if character.is_control() {
                return;
            }
        }
        // Only accept messages from people in our list.
        let person = match app.find_person(sender_id) {
            Some(person) => person,
            None => return,
        };

        if !self.already_received(sender_id, token) {
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
        self.send(&format!("RELAY {} {} RECEIPT", sender_id, token));
    }

    fn already_received(&self, sender_id: &str, token: &str) -> bool {
        for received in &self.received {
            if received.sender_id == sender_id && received.token == token {
                return true;
            }
        }
        return false;
    }

    fn update_status(&self, app: &mut App, now: Instant) {
        let mut connected = false;
        match self.last_server_reply {
            Some(reply_time) => {
                if now.duration_since(reply_time) < RELAY_SILENT_AFTER {
                    connected = true;
                }
            }
            None => {}
        }
        if connected {
            app.network_status = String::from("Relay connected");
        } else {
            app.network_status = String::from("Relay unavailable · retrying");
        }
    }

    // Move messages the user just sent from the app outbox into our pending list.
    fn queue_outgoing_messages(&mut self, app: &mut App, now: Instant) {
        while !app.outbox.is_empty() {
            let outgoing = app.outbox.remove(0);
            if self.pending.len() >= MAX_PENDING_MESSAGES {
                mark_delivery(app, &outgoing, Delivery::Unconfirmed);
                continue;
            }
            self.next_token_number += 1;
            self.pending.push(PendingMessage {
                outgoing: outgoing,
                token: format!("{}-{}", self.id, self.next_token_number),
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
                let packet = format!(
                    "RELAY {} {} CHAT {}",
                    self.pending[index].outgoing.person_id,
                    self.pending[index].token,
                    self.pending[index].outgoing.text
                );
                self.send(&packet);
                self.pending[index].next_send = now + RESEND_EVERY;
            }
            index += 1;
        }
    }

    fn forget_old_received_messages(&mut self, now: Instant) {
        let mut index = 0;
        while index < self.received.len() {
            if now.duration_since(self.received[index].received_at) >= REMEMBER_RECEIVED_FOR {
                self.received.remove(index);
            } else {
                index += 1;
            }
        }
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
    match person.messages.get_mut(outgoing.message_index) {
        Some(message) => {
            message.delivery = Some(delivery);
        }
        None => {}
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
