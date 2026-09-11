// Set up the terminal and restore it on exit or panic.

use std::io::{self, IsTerminal};

use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::execute;
use ratatui::DefaultTerminal;

pub fn start() -> io::Result<DefaultTerminal> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "run bit-to-byte in an interactive terminal",
        ));
    }

    // Install our paste cleanup first. Ratatui wraps this hook with its own
    // raw-mode and alternate-screen cleanup when try_init is called below.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_information| {
        let _cleanup_result = execute!(io::stdout(), DisableBracketedPaste);
        previous_hook(panic_information);
    }));

    let terminal = match ratatui::try_init() {
        Ok(terminal) => terminal,
        Err(error) => {
            let _cleanup_result = restore();
            return Err(error);
        }
    };

    match execute!(io::stdout(), EnableBracketedPaste) {
        Ok(()) => {}
        Err(error) => {
            let _cleanup_result = restore();
            return Err(error);
        }
    }

    return Ok(terminal);
}

pub fn restore() -> io::Result<()> {
    let paste_result = execute!(io::stdout(), DisableBracketedPaste);
    // Attempt both operations even if disabling paste fails.
    let terminal_result = ratatui::try_restore();

    match paste_result {
        Ok(()) => {}
        Err(error) => return Err(error),
    }

    return terminal_result;
}
