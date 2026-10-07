use crossterm::event::KeyCode;

use crate::app::state::{Delivery, Message, View};

use super::{draw_screen, press, sample_app};

#[test]
fn screens_handle_resizing_and_only_messages_allow_typing() {
    let mut app = sample_app();
    for _view in 0..2 {
        for (width, height) in [(0, 0), (1, 1), (39, 9), (40, 10), (64, 16), (100, 28)] {
            draw_screen(&app, width, height);
        }
        press(&mut app, KeyCode::Char('i'));
        assert_eq!(app.typing, app.view == View::Messages);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Tab);
    }
    app.network_status = String::from("Relay connected");
    let screen = draw_screen(&app, 80, 24);
    for removed in [
        "Bit to Byte",
        "Your club",
        "NOTICE",
        "[online]",
        "Relay connected",
        "You:",
        "↑",
        "↓",
    ] {
        assert!(!screen.contains(removed));
    }
    assert!(screen.contains("chat"));
    assert!(screen.contains("files"));
    assert!(draw_screen(&app, 39, 9).contains("Resize"));
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.running);
}

#[test]
fn delivery_labels_are_drawn_without_treating_message_text_as_metadata() {
    let mut app = sample_app();
    app.view = View::Messages;
    app.people[0].messages.clear();
    app.people[0]
        .messages
        .push(Message::received("You: still Maya"));
    let screen = draw_screen(&app, 80, 24);
    assert!(screen.contains("You: still Maya"));

    app.people[0].messages.push(Message::sent("hello"));
    assert!(draw_screen(&app, 80, 24).contains("you …"));
    app.people[0].messages[1].route = Some("D");
    assert!(draw_screen(&app, 80, 24).contains("you [D] …"));
    app.people[0].messages[1].delivery = Some(Delivery::Delivered);
    assert!(draw_screen(&app, 80, 24).contains("you [D]"));
    app.people[0].messages[1].route = Some("R");
    assert!(draw_screen(&app, 80, 24).contains("you [R]"));
    app.people[0].messages[1].delivery = Some(Delivery::Unconfirmed);
    assert!(draw_screen(&app, 80, 24).contains("you [R] ?"));
    app.selected_chat = 0;
    let mut received = Message::received("hello everyone");
    received.author = Some("Bob Smith".into());
    app.global_messages.push(received);
    app.people.clear();
    let global = draw_screen(&app, 80, 24);
    assert!(global.contains("Bob Smith"));
    assert!(global.contains("hello everyone"));
}

#[test]
fn newest_message_and_end_of_long_draft_stay_visible() {
    let mut app = sample_app();
    app.view = View::Messages;
    app.typing = true;
    for number in 0..50 {
        app.people[0]
            .messages
            .push(Message::received(&format!("Message {}", number)));
    }
    app.people[0].draft = format!("{}END", "界".repeat(100));
    let screen = draw_screen(&app, 64, 16);
    assert!(screen.contains("Message 49"));
    assert!(screen.contains("END"));
    app.typing = false;
    app.people[0].draft.clear();
    let last = app.people[0].messages.last_mut().unwrap();
    last.text = format!("{}WRAPPED", "word ".repeat(30));
    assert!(draw_screen(&app, 40, 10).contains("WRAPPED"));
}
