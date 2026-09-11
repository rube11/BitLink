use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::state::View;
use crate::tui::input;

use super::{draw_screen, press, sample_app};

#[test]
fn typing_keeps_shortcuts_as_text_and_only_adds_nonempty_messages() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.people[0].messages.len(), 1);
    press(&mut app, KeyCode::Char('q'));
    press(&mut app, KeyCode::Tab);
    assert!(app.running);
    assert_eq!(app.view, View::Messages);
    assert_eq!(app.people[0].draft, "q");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.people[0].messages[1], "You: q");
    assert_eq!(app.people[0].draft, "");
}

#[test]
fn changing_people_preserves_each_draft_and_message_recipient() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('b'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.people[1].messages[1], "You: b");
    assert_eq!(app.people[0].messages.len(), 1);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.people[app.selected_person].draft, "a");
}

#[test]
fn paste_is_bounded_and_cannot_submit_or_quit() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Tab);
    input::handle_event(&mut app, Event::Paste(String::from("ignored")));
    assert_eq!(app.people[0].draft, "");
    press(&mut app, KeyCode::Enter);
    input::handle_event(&mut app, Event::Paste(String::from("q\n界\t\u{1b}")));
    assert_eq!(app.people[0].draft, "q 界 ");
    assert_eq!(app.people[0].messages.len(), 1);
    assert!(app.running);
    input::handle_event(&mut app, Event::Paste("x".repeat(1000)));
    assert_eq!(app.people[0].draft.chars().count(), 500);
}

#[test]
fn backspace_is_utf8_safe_and_key_releases_are_ignored() {
    let mut app = sample_app();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Enter);
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
    press(&mut app, KeyCode::Tab);
    for _step in 0..10 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.selected_person, 2);
    for _step in 0..10 {
        press(&mut app, KeyCode::Up);
    }
    assert_eq!(app.selected_person, 0);
    app.people.clear();
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert!(!app.typing);
    assert!(draw_screen(&app, 80, 24).contains("No conversations yet"));
}
