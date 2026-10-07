// Application data. Every other module reads or changes this data.

use std::time::Instant;

pub const MAX_MESSAGE_CHARACTERS: usize = 500;
// Leave room for encryption and Base64 inside the relay's packet limit.
pub const MAX_MESSAGE_BYTES: usize = 1400;
// A global send needs one pending delivery per online member.
pub const MAX_PENDING_MESSAGES: usize = 128;

#[derive(Debug, PartialEq, Eq)]
pub enum View {
    Messages,
    Files,
}

// What we know about one of our own messages.
#[derive(Debug, PartialEq, Eq)]
pub enum Delivery {
    // Queued or sent, still waiting for the other app to confirm.
    Sending,
    // The other app confirmed that it received the message.
    Delivered,
    // No confirmation arrived in time. The message may still have arrived.
    Unconfirmed,
}

// One line in a conversation.
#[derive(Debug, PartialEq, Eq)]
pub struct Message {
    pub from_me: bool,
    pub text: String,
    // Global messages retain the sender name after they leave.
    pub author: Option<String>,
    // Global delivery is complete only when every recipient confirms.
    pub pending_receipts: usize,
    // Only our own messages have a delivery status.
    pub delivery: Option<Delivery>,
    // Last DM send attempt, or combined paths used by a global send.
    pub route: Option<&'static str>,
}

impl Message {
    pub fn received(text: &str) -> Message {
        return Message {
            from_me: false,
            text: String::from(text),
            author: None,
            pending_receipts: 0,
            delivery: None,
            route: None,
        };
    }

    pub fn sent(text: &str) -> Message {
        return Message {
            from_me: true,
            text: String::from(text),
            author: None,
            pending_receipts: 0,
            delivery: Some(Delivery::Sending),
            route: None,
        };
    }
}

pub struct Person {
    pub id: String,
    pub name: String,
    pub last_seen: Instant,
    pub messages: Vec<Message>,
    pub draft: String,
}

// A message the user pressed Enter on. The network module sends it later and
// updates the delivery status of the message at message_index.
pub struct OutgoingMessage {
    pub person_id: String,
    pub text: String,
    pub message_index: usize,
    pub kind: &'static str,
}

pub struct App {
    pub network_status: String,
    pub outbox: Vec<OutgoingMessage>,
    pub running: bool,
    pub typing: bool,
    pub view: View,
    pub people: Vec<Person>,
    // Row 0 is global; the remaining rows are online people.
    pub selected_chat: usize,
    pub global_messages: Vec<Message>,
    pub global_draft: String,
}

impl App {
    pub fn new() -> App {
        return App {
            network_status: String::from("Connecting to relay"),
            outbox: Vec::new(),
            running: true,
            typing: false,
            view: View::Messages,
            people: Vec::new(),
            selected_chat: 0,
            global_messages: Vec::new(),
            global_draft: String::new(),
        };
    }

    // Find a person by their network ID.
    pub fn find_person(&mut self, id: &str) -> Option<&mut Person> {
        for person in &mut self.people {
            if person.id == id {
                return Some(person);
            }
        }
        return None;
    }

    pub fn draft_mut(&mut self) -> Option<&mut String> {
        if self.selected_chat == 0 {
            return Some(&mut self.global_draft);
        }
        return self
            .people
            .get_mut(self.selected_chat - 1)
            .map(|person| &mut person.draft);
    }

    pub fn message_mut(&mut self, outgoing: &OutgoingMessage) -> Option<&mut Message> {
        if outgoing.kind == "GLOBAL" {
            return self.global_messages.get_mut(outgoing.message_index);
        }
        return self
            .find_person(&outgoing.person_id)?
            .messages
            .get_mut(outgoing.message_index);
    }

    pub fn finish_delivery(&mut self, outgoing: &OutgoingMessage, delivery: Delivery) {
        let Some(message) = self.message_mut(outgoing) else {
            return;
        };
        if outgoing.kind == "GLOBAL" {
            message.pending_receipts = message.pending_receipts.saturating_sub(1);
            if delivery == Delivery::Delivered
                && (message.pending_receipts > 0 || message.delivery == Some(Delivery::Unconfirmed))
            {
                return;
            }
        }
        message.delivery = Some(delivery);
    }

    pub fn remove_person(&mut self, index: usize) {
        let person = self.people.remove(index);
        for outgoing in std::mem::take(&mut self.outbox) {
            if outgoing.person_id == person.id {
                self.finish_delivery(&outgoing, Delivery::Unconfirmed);
            } else {
                self.outbox.push(outgoing);
            }
        }
        let row = index + 1;
        if row < self.selected_chat {
            self.selected_chat -= 1;
        } else if row == self.selected_chat {
            self.typing = false;
            self.selected_chat = row.min(self.people.len());
        }
    }
}
