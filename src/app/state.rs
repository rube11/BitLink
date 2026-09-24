// Application data. Every other module reads or changes this data.

use std::time::Instant;

pub const MAX_MESSAGE_CHARACTERS: usize = 500;
pub const MAX_PENDING_MESSAGES: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub enum View {
    Announcements,
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

impl Delivery {
    pub fn label(&self) -> &str {
        match self {
            Delivery::Sending => return "sending",
            Delivery::Delivered => return "delivered",
            Delivery::Unconfirmed => return "unconfirmed",
        }
    }
}

// One line in a conversation.
#[derive(Debug, PartialEq, Eq)]
pub struct Message {
    pub from_me: bool,
    pub text: String,
    // Only our own messages have a delivery status.
    pub delivery: Option<Delivery>,
}

impl Message {
    pub fn received(text: &str) -> Message {
        return Message {
            from_me: false,
            text: String::from(text),
            delivery: None,
        };
    }

    pub fn sent(text: &str) -> Message {
        return Message {
            from_me: true,
            text: String::from(text),
            delivery: Some(Delivery::Sending),
        };
    }
}

pub struct Person {
    pub id: String,
    pub name: String,
    pub online: bool,
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
}

pub struct App {
    pub name: String,
    pub network_status: String,
    pub outbox: Vec<OutgoingMessage>,
    pub running: bool,
    pub typing: bool,
    pub view: View,
    pub people: Vec<Person>,
    pub selected_person: usize,
    pub announcements: Vec<String>,
}

impl App {
    pub fn new() -> App {
        return App {
            name: String::from("Club member"),
            network_status: String::from("Connecting to relay"),
            outbox: Vec::new(),
            running: true,
            typing: false,
            view: View::Announcements,
            people: Vec::new(),
            selected_person: 0,
            announcements: vec![
                String::from("Welcome to Bit to Byte."),
                String::from("Open the app with internet access to join the people list."),
                String::from("People stay listed as offline when they leave."),
                String::from(
                    "Demo chat is relayed without encryption. History lasts until you quit.",
                ),
            ],
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
}
