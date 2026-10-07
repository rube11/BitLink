// Registration, discovery, and opaque forwarding, shared by chat and QUIC.
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

pub(super) const REGISTRATION_LIFETIME: Duration = Duration::from_secs(30);
pub(super) const PRESENCE_LIFETIME: Duration = Duration::from_secs(6);
pub(super) const MAX_MEMBERS: usize = 128;
pub(super) const MAX_PAYLOAD_BYTES: usize = 2100;

pub(super) struct Member {
    id: String,
    name: String,
    address: SocketAddr,
    last_seen: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reply {
    pub address: SocketAddr,
    pub text: String,
}

#[derive(Default)]
pub(super) struct Server {
    pub(super) members: Vec<Member>,
}

impl Server {
    pub(super) fn handle(&mut self, text: &str, source: SocketAddr, now: Instant) -> Vec<Reply> {
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

    pub(super) fn member_index(&self, id: &str) -> Option<usize> {
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

// Keep QUIC binary: Base64 would push a 1200-byte packet over the network MTU.
#[derive(Default)]
pub(super) struct Relay {
    chat: Server,
    sharing: Server,
}

impl Relay {
    pub fn handle(
        &mut self,
        packet: &[u8],
        source: SocketAddr,
        now: Instant,
    ) -> Vec<(SocketAddr, Vec<u8>)> {
        if let Some(body) = packet.strip_prefix(b"QUIC DATA ") {
            self.sharing
                .members
                .retain(|member| now.duration_since(member.last_seen) < REGISTRATION_LIFETIME);
            let Some(split) = body.iter().position(|byte| *byte == b' ') else {
                return Vec::new();
            };
            let Ok(id) = std::str::from_utf8(&body[..split]) else {
                return Vec::new();
            };
            let data = &body[split + 1..];
            if data.is_empty() || data.len() > 1200 {
                return Vec::new();
            }
            let Some(sender) = self
                .sharing
                .members
                .iter()
                .find(|member| member.address == source)
            else {
                return Vec::new();
            };
            let Some(recipient) = self
                .sharing
                .members
                .iter()
                .find(|member| member.id == id && member.address != source)
            else {
                return Vec::new();
            };
            let mut forwarded = format!("QUIC FROM {} ", sender.id).into_bytes();
            forwarded.extend_from_slice(data);
            return vec![(recipient.address, forwarded)];
        }
        let Ok(text) = std::str::from_utf8(packet) else {
            return Vec::new();
        };
        let (service, prefix, text) = match text.strip_prefix("QUIC ") {
            Some(text) => (&mut self.sharing, "QUIC ", text),
            None => (&mut self.chat, "", text),
        };
        service
            .handle(text, source, now)
            .into_iter()
            .map(|reply| {
                (
                    reply.address,
                    format!("{prefix}{}", reply.text).into_bytes(),
                )
            })
            .collect()
    }
}
