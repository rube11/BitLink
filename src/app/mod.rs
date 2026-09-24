// Application startup and the event loop.
pub mod network;
pub mod presence;
pub mod state;

use std::io;
use std::time::Duration;

use crossterm::event;

use crate::app::network::Network;
use crate::app::state::App;
use crate::tui::{input, render, terminal};

pub fn run() -> io::Result<()> {
    let mut app = App::new();
    let mut chosen_name = None;
    let mut arguments = std::env::args();
    arguments.next();
    loop {
        let argument = match arguments.next() {
            Some(argument) => argument,
            None => break,
        };
        match argument.as_str() {
            "--help" => {
                println!("Usage: bit-to-byte [--name NAME]");
                return Ok(());
            }
            "--name" => {
                chosen_name = match arguments.next() {
                    Some(name) => Some(name.trim().to_string()),
                    None => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Missing name. Use --name Alice.",
                        ));
                    }
                };
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Unknown option. Use --help.",
                ));
            }
        }
    }
    let mut network = match Network::connect(chosen_name) {
        Ok(network) => network,
        Err(error) => return Err(error),
    };
    app.name = String::from(network.name());
    let mut terminal = match terminal::start() {
        Ok(terminal) => terminal,
        Err(error) => return Err(error),
    };
    let run_result = run_event_loop(&mut terminal, &mut app, &mut network);
    // A lost goodbye is handled by registration freshness and client timeouts.
    network.goodbye();
    // Restore the terminal even when drawing or reading a key fails.
    let restore_result = terminal::restore();

    match run_result {
        Ok(()) => {
            return restore_result;
        }
        Err(error) => {
            match restore_result {
                Ok(()) => {}
                Err(restore_error) => {
                    eprintln!("Could not restore the terminal: {}", restore_error);
                }
            }
            return Err(error);
        }
    }
}

fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    network: &mut Network,
) -> io::Result<()> {
    while app.running {
        network.update(app);
        // Ratatui needs a callback so it can provide the frame to draw on.
        let draw_result = terminal.draw(|frame| {
            render::draw(frame, app);
        });
        match draw_result {
            Ok(_) => {}
            Err(error) => return Err(error),
        }

        // Wait between redraws so resizing works even without a keypress.
        let event_is_ready = match event::poll(Duration::from_millis(200)) {
            Ok(event_is_ready) => event_is_ready,
            Err(error) => return Err(error),
        };
        if !event_is_ready {
            continue;
        }

        let event = match event::read() {
            Ok(event) => event,
            Err(error) => return Err(error),
        };
        input::handle_event(app, event);
    }

    return Ok(());
}
