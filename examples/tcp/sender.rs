// Connect to a receiver and send one line each time the user presses Enter.

use std::env;
use std::io::{self, Write};
use std::net::TcpStream;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = env::args();
    let _program_name = arguments.next();
    let receiver_address = match arguments.next() {
        Some(address) => address,
        None => String::from("127.0.0.1:7000"),
    };

    if arguments.next().is_some() {
        eprintln!("Usage: sender [receiver_ip:port]");
        return ExitCode::FAILURE;
    }

    let mut stream = match TcpStream::connect(&receiver_address) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("Could not connect to {}: {}", receiver_address, error);
            return ExitCode::FAILURE;
        }
    };

    println!("Connected to {}.", receiver_address);
    println!("Type a message and press Enter. Type /quit to disconnect.");

    let keyboard = io::stdin();
    loop {
        let mut message = String::new();
        let bytes_read = match keyboard.read_line(&mut message) {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                eprintln!("Could not read your message: {}", error);
                return ExitCode::FAILURE;
            }
        };

        if bytes_read == 0 || message.trim_end() == "/quit" {
            return ExitCode::SUCCESS;
        }

        // Piped input can end without a newline. Every network message needs one.
        if !message.ends_with('\n') {
            message.push('\n');
        }

        // write_all keeps writing until the entire message is handed to TCP or fails.
        match stream.write_all(message.as_bytes()) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("Could not send to {}: {}", receiver_address, error);
                return ExitCode::FAILURE;
            }
        }
    }
}
