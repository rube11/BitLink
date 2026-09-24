// Keep the people list up to date from hello and goodbye packets.
//
// The relay sends a four-line hello packet for every named person it knows,
// and a goodbye packet when someone quits:
//
//     bit-to-byte/1
//     hello
//     <person id>
//     <display name>

use std::time::{Duration, Instant};

use crate::app::state::{App, Person};

// Someone whose hello has not been repeated for this long is shown offline.
pub const OFFLINE_AFTER: Duration = Duration::from_secs(8);

const PROTOCOL_LINE: &str = "bit-to-byte/1";
const MAX_NAME_CHARACTERS: usize = 40;
const MAX_ID_BYTES: usize = 64;

pub fn is_valid_name(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    if name.chars().count() > MAX_NAME_CHARACTERS {
        return false;
    }
    for character in name.chars() {
        if character.is_control() {
            return false;
        }
    }
    return true;
}

pub fn is_presence_packet(text: &str) -> bool {
    return text.starts_with("bit-to-byte/1\n");
}

pub fn receive_packet(app: &mut App, text: &str, own_id: &str, now: Instant) {
    let mut lines: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        lines.push(line);
    }
    if lines.len() != 4 {
        return;
    }
    if lines[0] != PROTOCOL_LINE {
        return;
    }
    let packet_type = lines[1];
    let person_id = lines[2];
    let name = lines[3];

    if packet_type != "hello" && packet_type != "goodbye" {
        return;
    }
    if person_id.is_empty() || person_id.len() > MAX_ID_BYTES {
        return;
    }
    if !is_valid_name(name) {
        return;
    }
    // The relay may echo our own registration. We should not list ourselves.
    if person_id == own_id {
        return;
    }

    let is_hello = packet_type == "hello";

    match app.find_person(person_id) {
        Some(person) => {
            person.online = is_hello;
            person.last_seen = now;
        }
        None => {
            // A goodbye from someone we never saw should not create a conversation.
            if is_hello {
                app.people.push(Person {
                    id: String::from(person_id),
                    name: String::from(name),
                    online: true,
                    last_seen: now,
                    messages: Vec::new(),
                    draft: String::new(),
                });
            }
        }
    }
}

pub fn mark_missing_people_offline(app: &mut App, now: Instant) {
    for person in &mut app.people {
        if now.duration_since(person.last_seen) >= OFFLINE_AFTER {
            person.online = false;
        }
    }
}
