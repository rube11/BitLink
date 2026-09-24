// Keep a list of registered people and forward packets between them.
// Messages, receipts, retries, and conversations belong to the clients.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const REGISTRATION_LIFETIME: Duration = Duration::from_secs(30);
const PRESENCE_LIFETIME: Duration = Duration::from_secs(6);
const MAX_MEMBERS: usize = 128;
const MAX_PAYLOAD_BYTES: usize = 2100;

struct Member {
    id: String,
    name: String,
    address: SocketAddr,
    last_seen: Instant,
}

#[derive(Debug, PartialEq, Eq)]
struct Reply {
    address: SocketAddr,
    text: String,
}

struct Server {
    members: Vec<Member>,
}

impl Server {
    fn new() -> Server {
        return Server {
            members: Vec::new(),
        };
    }

    fn handle(&mut self, text: &str, source: SocketAddr, now: Instant) -> Vec<Reply> {
        self.remove_expired_members(now);

        // Keep spaces in display names and message bodies.
        let mut words = Vec::new();
        for word in text.splitn(3, ' ') {
            words.push(word);
        }
        if words.len() < 2 {
            return Vec::new();
        }
        let command = words[0];
        let person_id = words[1];
        let mut body = "";
        if words.len() == 3 {
            body = words[2];
        }
        if !valid_id(person_id) {
            return Vec::new();
        }

        match command {
            "REGISTER" => return self.register(person_id, body, source, now),
            "RELAY" => return self.relay(person_id, body, source),
            "GOODBYE" => {
                if body.is_empty() {
                    return self.goodbye(person_id, source);
                }
            }
            _ => {}
        }
        return Vec::new();
    }

    fn register(&mut self, id: &str, name: &str, source: SocketAddr, now: Instant) -> Vec<Reply> {
        if !valid_name(name) {
            return Vec::new();
        }
        let mut replies = Vec::new();

        for member in &self.members {
            if member.id == id && member.address != source {
                replies.push(Reply {
                    address: source,
                    text: String::from("ERROR id-in-use"),
                });
                return replies;
            }
            if member.address == source && member.id != id {
                replies.push(Reply {
                    address: source,
                    text: String::from("ERROR address-in-use"),
                });
                return replies;
            }
        }

        match self.member_index(id) {
            Some(index) => {
                self.members[index].name = String::from(name);
                self.members[index].last_seen = now;
            }
            None => {
                if self.members.len() >= MAX_MEMBERS {
                    return replies;
                }
                self.members.push(Member {
                    id: String::from(id),
                    name: String::from(name),
                    address: source,
                    last_seen: now,
                });
                println!("Registered {} observed={}", id, source);
            }
        }

        replies.push(Reply {
            address: source,
            text: format!("REGISTERED {}", source),
        });
        for member in &self.members {
            if member.id != id && now.duration_since(member.last_seen) < PRESENCE_LIFETIME {
                replies.push(Reply {
                    address: source,
                    text: format!("bit-to-byte/1\nhello\n{}\n{}", member.id, member.name),
                });
            }
        }
        return replies;
    }

    fn goodbye(&mut self, id: &str, source: SocketAddr) -> Vec<Reply> {
        let index = match self.member_index(id) {
            Some(index) => index,
            None => return Vec::new(),
        };
        if self.members[index].address != source {
            return Vec::new();
        }

        let leaving = self.members.remove(index);
        let mut replies = Vec::new();
        for member in &self.members {
            replies.push(Reply {
                address: member.address,
                text: format!("bit-to-byte/1\ngoodbye\n{}\n{}", leaving.id, leaving.name),
            });
        }
        return replies;
    }

    fn relay(&self, recipient_id: &str, payload: &str, source: SocketAddr) -> Vec<Reply> {
        if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
            return Vec::new();
        }
        // The sender is identified by its registered address, not by a packet claim.
        let mut sender_id = None;
        for member in &self.members {
            if member.address == source {
                sender_id = Some(member.id.as_str());
                break;
            }
        }
        let sender_id = match sender_id {
            Some(id) => id,
            None => return Vec::new(),
        };

        let mut replies = Vec::new();
        match self.member_index(recipient_id) {
            Some(index) => {
                let recipient = &self.members[index];
                if recipient.address != source {
                    replies.push(Reply {
                        address: recipient.address,
                        text: format!("FROM {} {}", sender_id, payload),
                    });
                    println!("Relaying {} -> {}", sender_id, recipient_id);
                    return replies;
                }
            }
            None => {}
        }
        replies.push(Reply {
            address: source,
            text: String::from("ERROR peer-unavailable"),
        });
        return replies;
    }

    fn member_index(&self, id: &str) -> Option<usize> {
        let mut index = 0;
        while index < self.members.len() {
            if self.members[index].id == id {
                return Some(index);
            }
            index += 1;
        }
        return None;
    }

    fn remove_expired_members(&mut self, now: Instant) {
        let mut index = 0;
        while index < self.members.len() {
            if now.duration_since(self.members[index].last_seen) >= REGISTRATION_LIFETIME {
                self.members.remove(index);
            } else {
                index += 1;
            }
        }
    }
}

fn valid_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 40 {
        return false;
    }
    for byte in id.bytes() {
        if !byte.is_ascii_alphanumeric() && byte != b'-' && byte != b'_' {
            return false;
        }
    }
    return true;
}

fn valid_name(name: &str) -> bool {
    if name.trim().is_empty() || name.chars().count() > 40 {
        return false;
    }
    for character in name.chars() {
        if character.is_control() {
            return false;
        }
    }
    return true;
}

fn main() -> io::Result<()> {
    let address = match std::env::args().nth(1) {
        Some(address) => address,
        None => String::from("0.0.0.0:47002"),
    };
    let socket = match UdpSocket::bind(&address) {
        Ok(socket) => socket,
        Err(error) => return Err(error),
    };
    println!("Presence and chat relay listening on {}", address);

    let mut server = Server::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let (count, source) = match socket.recv_from(&mut buffer) {
            Ok(packet) => packet,
            Err(error) => {
                eprintln!("Receive failed: {}", error);
                continue;
            }
        };
        // A full buffer may mean that the packet was cut short.
        if count == buffer.len() {
            continue;
        }
        let text = match std::str::from_utf8(&buffer[..count]) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let replies = server.handle(text, source, Instant::now());
        for reply in replies {
            match socket.send_to(reply.text.as_bytes(), reply.address) {
                Ok(_) => {}
                Err(error) => eprintln!("Send failed: {}", error),
            }
        }
    }
}

#[cfg(test)]
mod tests;
