use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::presence::{OFFLINE_AFTER, receive_packet, remove_missing_people};
use crate::app::state::{MAX_MESSAGE_BYTES, Message, View};
use crate::tui::input;

use super::{draw_screen, press, sample_app};

#[test]
fn typing_keeps_shortcuts_as_text_and_only_adds_nonempty_messages() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Char('l'));
    press(&mut app, KeyCode::Enter);
    assert!(!app.typing);
    press(&mut app, KeyCode::Char('i'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.people[0].messages.len(), 1);
    for character in "qjkli".chars() {
        press(&mut app, KeyCode::Char(character));
    }
    press(&mut app, KeyCode::Tab);
    assert!(app.running);
    assert_eq!(app.view, View::Messages);
    assert_eq!(app.people[0].draft, "qjkli");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.people[0].messages[1], Message::sent("qjkli"));
    assert_eq!(app.people[0].draft, "");
}

#[test]
fn changing_people_preserves_each_draft_and_message_recipient() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Char('i'));
    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('i'));
    press(&mut app, KeyCode::Char('b'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.people[1].messages[1], Message::sent("b"));
    assert_eq!(app.outbox.len(), 1);
    assert_eq!(app.outbox[0].person_id, app.people[1].id);
    assert_eq!(app.outbox[0].text, "b");
    assert_eq!(app.outbox[0].message_index, 1);
    assert_eq!(app.people[0].messages.len(), 1);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.people[app.selected_chat - 1].draft, "a");
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Char('i'));
    input::handle_event(&mut app, Event::Paste("group draft".into()));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.people[0].draft, "a");
    assert_eq!(app.global_draft, "group draft");
}

#[test]
fn removing_people_preserves_selection_and_stops_typing_when_the_recipient_leaves() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('i'));
    press(&mut app, KeyCode::Char('a'));
    let now = std::time::Instant::now();
    receive_packet(&mut app, "bit-to-byte/1\ngoodbye\nMaya\nMaya", "self", now);
    assert_eq!(app.selected_chat, 1);
    assert_eq!(app.people[0].id, "Alex");
    assert_eq!(app.people[0].draft, "a");
    assert!(app.typing);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.outbox.len(), 1);
    receive_packet(&mut app, "bit-to-byte/1\ngoodbye\nAlex\nAlex", "self", now);
    assert!(!app.typing);
    assert_eq!(app.people[app.selected_chat - 1].id, "Sam");
    assert!(app.outbox.is_empty());
    press(&mut app, KeyCode::Char('x'));
    assert!(app.people[0].draft.is_empty());
    remove_missing_people(&mut app, now + OFFLINE_AFTER);
    assert!(app.people.is_empty());
    assert_eq!(app.selected_chat, 0);
    assert!(draw_screen(&app, 80, 24).contains("No one online"));
}

#[test]
fn paste_is_bounded_and_cannot_submit_or_quit() {
    let mut app = sample_app();
    input::handle_event(&mut app, Event::Paste(String::from("ignored")));
    assert_eq!(app.people[0].draft, "");
    press(&mut app, KeyCode::Char('i'));
    input::handle_event(&mut app, Event::Paste(String::from("q\n界\t\u{1b}")));
    assert_eq!(app.people[0].draft, "q 界 ");
    assert_eq!(app.people[0].messages.len(), 1);
    assert!(app.running);
    input::handle_event(&mut app, Event::Paste("x".repeat(1000)));
    assert_eq!(app.people[0].draft.chars().count(), 500);
    app.people[0].draft.clear();
    input::handle_event(&mut app, Event::Paste("😀".repeat(500)));
    assert_eq!(app.people[0].draft.len(), MAX_MESSAGE_BYTES);
    assert_eq!(app.people[0].draft.chars().count(), 350);
}

#[test]
fn backspace_is_utf8_safe_and_key_releases_are_ignored() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Char('i'));
    press(&mut app, KeyCode::Char('界'));
    let released_key = KeyEvent::new_with_kind(
        KeyCode::Char('界'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    input::handle_event(&mut app, Event::Key(released_key));
    assert_eq!(app.people[0].draft, "界");
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    assert_eq!(app.people[0].draft, "");
    let quit_key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    input::handle_event(&mut app, Event::Key(quit_key));
    assert!(!app.running);
}

#[test]
fn selection_handles_list_edges_and_an_empty_list() {
    let mut app = sample_app();
    for _step in 0..10 {
        press(&mut app, KeyCode::Char('j'));
    }
    assert_eq!(app.selected_chat, 3);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected_chat, 3);
    for _step in 0..10 {
        press(&mut app, KeyCode::Char('k'));
    }
    assert_eq!(app.selected_chat, 0);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.selected_chat, 0);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.view, View::Files);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('i'));
    assert_eq!(app.selected_chat, 0);
    assert!(!app.typing);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.view, View::Messages);
    app.people.clear();
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('i'));
    assert!(!app.typing);
    assert!(draw_screen(&app, 80, 24).contains("No one online"));
}
