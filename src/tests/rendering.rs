use crossterm::event::KeyCode;

use crate::app::state::View;

use super::{draw_screen, press, sample_app};

#[test]
fn screens_handle_resizing_and_only_messages_allow_typing() {
    let mut app = sample_app();
    for _view in 0..3 {
        for (width, height) in [(0, 0), (1, 1), (40, 10), (64, 16), (100, 28)] {
            draw_screen(&app, width, height);
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.typing, app.view == View::Messages);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Tab);
    }
    assert!(draw_screen(&app, 80, 24).contains("Bit to Byte"));
    assert!(draw_screen(&app, 40, 10).contains("Resize"));
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.running);
}

#[test]
fn newest_message_and_end_of_long_draft_stay_visible() {
    let mut app = sample_app();
    app.view = View::Messages;
    app.typing = true;
    for number in 0..50 {
        app.people[0].messages.push(format!("Message {}", number));
    }
    app.people[0].draft = format!("{}END", "界".repeat(100));
    let screen = draw_screen(&app, 64, 16);
    assert!(screen.contains("Message 49"));
    assert!(screen.contains("END"));
}
