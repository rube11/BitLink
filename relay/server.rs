// Register people, introduce their observed endpoints, and forward fallback traffic.
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

#[derive(Default)]
struct Server {
    members: Vec<Member>,
}

impl Server {
    fn handle(&mut self, text: &str, source: SocketAddr, now: Instant) -> Vec<Reply> {
        self.members.retain(|member| {
            return now.duration_since(member.last_seen) < REGISTRATION_LIFETIME;
        });

        // Keep spaces in display names and message bodies.
        let mut words = text.splitn(3, ' ');
        let (command, person_id) = match (words.next(), words.next()) {
            (Some(command), Some(id)) => (command, id),
            _ => return Vec::new(),
        };
        if person_id.is_empty()
            || person_id.len() > 40
            || !person_id.bytes().all(|byte| {
                return byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
            })
        {
            return Vec::new();
        }
        let body = words.next().unwrap_or("");
        match command {
            "REGISTER" => return self.register(person_id, body, source, now),
            "RELAY" => return self.relay(person_id, body, source),
            "DISCOVER" if body.is_empty() => return self.discover(person_id, source, now),
            "GOODBYE" if body.is_empty() => return self.goodbye(person_id, source),
            _ => return Vec::new(),
        }
    }

    fn register(&mut self, id: &str, name: &str, source: SocketAddr, now: Instant) -> Vec<Reply> {
        if name.trim().is_empty() || name.chars().count() > 40 || name.chars().any(char::is_control)
        {
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
        }
        // A new run can reuse the previous run's endpoint after a lost goodbye.
        // Traffic from that endpoint replaces its old session, not another address's ID.
        if let Some(index) = self
            .members
            .iter()
            .position(|member| member.address == source && member.id != id)
        {
            let old_id = self.members[index].id.clone();
            replies.extend(self.goodbye(&old_id, source));
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

    fn discover(&self, recipient_id: &str, source: SocketAddr, now: Instant) -> Vec<Reply> {
        // Introduce only registered, recently active clients. Never accept an
        // address from the request: both endpoints come from observed traffic.
        let mut sender = None;
        let mut recipient = None;
        for member in &self.members {
            if now.duration_since(member.last_seen) >= PRESENCE_LIFETIME {
                continue;
            }
            if member.address == source {
                sender = Some(member);
            }
            if member.id == recipient_id {
                recipient = Some(member);
            }
        }
        let (sender, recipient) = match (sender, recipient) {
            (Some(sender), Some(recipient)) if sender.id != recipient.id => (sender, recipient),
            _ => return Vec::new(),
        };
        return vec![
            Reply {
                address: sender.address,
                text: format!(
                    "PEER {} {} {}",
                    recipient.id, recipient.address, recipient.name
                ),
            },
            Reply {
                address: recipient.address,
                text: format!("PEER {} {} {}", sender.id, sender.address, sender.name),
            },
        ];
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

    let mut server = Server::default();
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
