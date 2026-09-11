// Application data. Discovery adds people as their automatic hello packets arrive.

use std::time::Instant;

#[derive(Debug, PartialEq, Eq)]
pub enum View {
    Announcements,
    Messages,
    Files,
}

pub struct Person {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub last_seen: Instant,
    pub messages: Vec<String>,
    pub draft: String,
}

pub struct App {
    pub name: String,
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
            running: true,
            typing: false,
            view: View::Announcements,
            people: Vec::new(),
            selected_person: 0,
            announcements: vec![
                String::from("Welcome to Bit to Byte."),
                String::from("Open the app on the same network to appear in the people list."),
                String::from("People stay listed as offline when they leave."),
                String::from("Messages stay on this computer until you quit."),
            ],
        };
    }
}
