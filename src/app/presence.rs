// Read automatic discovery packets and update the people list.

use std::time::{Duration, Instant};

use crate::app::state::{App, Person};

pub const OFFLINE_AFTER: Duration = Duration::from_secs(8);

pub fn is_valid_name(name: &str) -> bool {
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

pub fn receive_packet(app: &mut App, bytes: &[u8], own_id: &str, now: Instant) {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return,
    };
    let mut lines = Vec::new();
    for line in text.split('\n') {
        lines.push(line);
    }
    // Each packet has four lines: protocol, hello/goodbye, instance ID, name.
    if lines.len() != 4 {
        return;
    }
    if lines[0] != "bit-to-byte/1" {
        return;
    }
    let message_type = lines[1];
    let person_id = lines[2];
    let name = lines[3];

    if message_type != "hello" && message_type != "goodbye" {
        return;
    }
    if person_id.is_empty() || person_id.len() > 64 {
        return;
    }
    if !is_valid_name(name) {
        return;
    }
    // We receive our own packets too, but should not list ourselves.
    if person_id == own_id {
        return;
    }

    for person in &mut app.people {

        // update online status.    update when last heard
        // stop processring thios packet
    }

    // A goodbye from someone we never saw should not create a conversation.
    if message_type == "hello" {
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

pub fn mark_missing_people_offline(app: &mut App, now: Instant) {
    for person in &mut app.people {
        if now.duration_since(person.last_seen) >= OFFLINE_AFTER {
            person.online = false;
        }
    }
}
