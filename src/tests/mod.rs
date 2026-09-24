// Shared helpers for keyboard and rendering tests.
mod keyboard;
mod presence;
mod rendering;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::state::{App, Message, Person};
use crate::tui::{input, render};

fn sample_app() -> App {
    let mut app = App::new();
    for name in ["Maya", "Alex", "Sam"] {
        app.people.push(Person {
            id: String::from(name),
            name: String::from(name),
            online: true,
            last_seen: std::time::Instant::now(),
            messages: vec![Message::received("This is a sample conversation.")],
            draft: String::new(),
        });
    }
    return app;
}

fn press(app: &mut App, code: KeyCode) {
    let key = KeyEvent::new(code, KeyModifiers::NONE);
    input::handle_event(app, Event::Key(key));
}

fn draw_screen(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => panic!("Could not create test terminal: {}", error),
    };
    let result = terminal.draw(|frame| {
        render::draw(frame, app);
    });
    match result {
        Ok(_) => {}
        Err(error) => panic!("Could not draw test screen: {}", error),
    }

    let mut screen = String::new();
    for row in 0..height {
        for column in 0..width {
            screen.push_str(terminal.backend().buffer()[(column, row)].symbol());
        }
        screen.push('\n');
    }
    return screen;
}
