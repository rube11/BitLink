// Read a terminal event and change the application data.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::state::{
    App, MAX_MESSAGE_CHARACTERS, MAX_PENDING_MESSAGES, Message, OutgoingMessage, View,
};

pub fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) => {
            handle_key(app, key);
        }
        Event::Paste(text) => {
            if app.typing {
                match app.people.get_mut(app.selected_person) {
                    Some(person) => {
                        append_text(&mut person.draft, &text);
                    }
                    None => {}
                }
            }
        }
        _ => {} // Resize events cause a redraw on the next loop iteration.
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    // Ignore release events so a key does not get handled twice on Windows.
    if key.kind != KeyEventKind::Press {
        return;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if key.code == KeyCode::Char('c') {
            app.running = false;
        }
        return;
    }

    if key.modifiers.contains(KeyModifiers::ALT) || key.modifiers.contains(KeyModifiers::SUPER) {
        return;
    }

    if app.typing {
        handle_typing(app, key.code);
        return;
    }

    match key.code {
        KeyCode::Char('q') => {
            app.running = false;
        }
        KeyCode::Tab => match app.view {
            View::Announcements => {
                app.view = View::Messages;
            }
            View::Messages => {
                app.view = View::Files;
            }
            View::Files => {
                app.view = View::Announcements;
            }
        },
        KeyCode::Up => {
            if app.view == View::Messages && app.selected_person > 0 {
                app.selected_person -= 1;
            }
        }
        KeyCode::Down => {
            if app.view == View::Messages && app.selected_person + 1 < app.people.len() {
                app.selected_person += 1;
            }
        }
        KeyCode::Enter => {
            if app.view == View::Messages && app.people.get(app.selected_person).is_some() {
                app.typing = true;
            }
        }
        _ => {}
    }
}

fn handle_typing(app: &mut App, key: KeyCode) {
    if key == KeyCode::Esc {
        app.typing = false;
        return;
    }

    let person = match app.people.get_mut(app.selected_person) {
        Some(person) => person,
        None => return,
    };

    match key {
        KeyCode::Char(character) => {
            append_text(&mut person.draft, &character.to_string());
        }
        KeyCode::Backspace => {
            person.draft.pop();
        }
        KeyCode::Enter => {
            let message = person.draft.trim();
            if !message.is_empty() {
                if !person.online || app.outbox.len() >= MAX_PENDING_MESSAGES {
                    return;
                }
                app.outbox.push(OutgoingMessage {
                    person_id: person.id.clone(),
                    text: String::from(message),
                    message_index: person.messages.len(),
                });
                person.messages.push(Message::sent(message));
                person.draft.clear();
            }
        }
        _ => {}
    }
}

fn append_text(draft: &mut String, text: &str) {
    let mut character_count = draft.chars().count();
    for character in text.chars() {
        if character_count >= MAX_MESSAGE_CHARACTERS {
            return;
        }

        // Pasted newlines become spaces, rather than submitting the message.
        if character == '\n' || character == '\r' || character == '\t' {
            draft.push(' ');
        } else if !character.is_control() {
            draft.push(character);
        } else {
            continue;
        }
        character_count += 1;
    }
}
