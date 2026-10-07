// Read a terminal event and change the application data.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::sharing::Command;
use crate::app::state::{
    App, MAX_MESSAGE_BYTES, MAX_MESSAGE_CHARACTERS, MAX_PENDING_MESSAGES, Message, OutgoingMessage,
    View,
};

pub fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Key(key) => {
            handle_key(app, key);
        }
        Event::Paste(text) => {
            if app.typing
                && let Some(draft) = app.draft_mut()
            {
                append_text(draft, &text);
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
        KeyCode::Tab => {
            app.view = match app.view {
                View::Messages => View::Files,
                View::Files => View::Messages,
            }
        }
        KeyCode::Char('k') if app.view == View::Messages => {
            app.selected_chat = app.selected_chat.saturating_sub(1);
        }
        KeyCode::Char('j')
            if app.view == View::Messages && app.selected_chat < app.people.len() =>
        {
            app.selected_chat += 1;
        }
        KeyCode::Char('i') => {
            if app.view == View::Messages && !app.people.is_empty() {
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

    let Some(draft) = app.draft_mut() else {
        return;
    };

    match key {
        KeyCode::Char(character) => {
            append_text(draft, &character.to_string());
        }
        KeyCode::Backspace => {
            draft.pop();
        }
        KeyCode::Enter => {
            let message = draft.trim().to_string();
            if message.starts_with('/') {
                let words: Vec<_> = message.split_whitespace().collect();
                let person_id = app
                    .selected_chat
                    .checked_sub(1)
                    .and_then(|index| app.people.get(index))
                    .map(|person| person.id.clone());
                let port = words
                    .get(1)
                    .map_or(Some(7331), |port| port.parse::<u16>().ok())
                    .filter(|port| *port != 0);
                app.sharing.command = match (words.as_slice(), port) {
                    (["/stop"], _) => Some(Command::Stop),
                    (["/share", _], Some(port)) if !app.people.is_empty() => {
                        Some(Command::Host { person_id, port })
                    }
                    (["/accept"] | ["/accept", _], Some(port)) => app
                        .sharing
                        .offers
                        .iter()
                        .rev()
                        .find(|(id, offer)| {
                            person_id.as_ref().map_or(offer.global, |peer| peer == id)
                        })
                        .cloned()
                        .map(|(person_id, offer)| Command::Join {
                            person_id,
                            offer,
                            port,
                        }),
                    _ => None,
                };
                if app.sharing.command.is_some() {
                    app.draft_mut().unwrap().clear();
                    app.typing = false;
                } else {
                    app.sharing.status = "/share PORT · /accept [PORT] · /stop".into();
                }
                return;
            }
            let recipients: Vec<String> = if app.selected_chat == 0 {
                app.people.iter().map(|person| person.id.clone()).collect()
            } else {
                vec![app.people[app.selected_chat - 1].id.clone()]
            };
            if message.is_empty()
                || recipients.is_empty()
                || app.outbox.len() + recipients.len() > MAX_PENDING_MESSAGES
            {
                return;
            }
            let messages = if app.selected_chat == 0 {
                &mut app.global_messages
            } else {
                &mut app.people[app.selected_chat - 1].messages
            };
            let message_index = messages.len();
            let mut sent = Message::sent(&message);
            if app.selected_chat == 0 {
                sent.pending_receipts = recipients.len();
            }
            messages.push(sent);
            for person_id in recipients {
                app.outbox.push(OutgoingMessage {
                    person_id,
                    text: message.clone(),
                    message_index,
                    kind: if app.selected_chat == 0 {
                        "GLOBAL"
                    } else {
                        "CHAT"
                    },
                });
            }
            if let Some(draft) = app.draft_mut() {
                draft.clear();
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
        if draft.len() + character.len_utf8() > MAX_MESSAGE_BYTES {
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
