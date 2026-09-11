// Listen for a sender, print its messages, then wait for the next sender.

use std::env;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut arguments = env::args();
    let _program_name = arguments.next();
    let listen_address = match arguments.next() {
        Some(address) => address,
        None => String::from("0.0.0.0:7000"),
    };

    if arguments.next().is_some() {
        eprintln!("Usage: receiver [listen_ip:port]");
        return ExitCode::FAILURE;
    }

    // 0.0.0.0 listens on all IPv4 interfaces, so another laptop can connect.
    let listener = match TcpListener::bind(&listen_address) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("Could not listen on {}: {}", listen_address, error);
            return ExitCode::FAILURE;
        }
    };

    let bound_address = match listener.local_addr() {
        Ok(address) => address,
        Err(error) => {
            eprintln!("Could not read the listening address: {}", error);
            return ExitCode::FAILURE;
        }
    };
    println!("Listening on {}. Press Ctrl+C to stop.", bound_address);

    loop {
        // This waits until a sender connects. Only one sender is handled at a time.
        let (stream, sender_address) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("Could not accept a connection: {}", error);
                return ExitCode::FAILURE;
            }
        };
        println!("Connected: {}", sender_address);

        // TCP delivers bytes. BufReader collects them until a newline ends the message.
        let mut reader = BufReader::new(stream);
        loop {
            let mut message = String::new();
            let bytes_read = match reader.read_line(&mut message) {
                Ok(bytes_read) => bytes_read,
                Err(error) => {
                    eprintln!("Could not read from {}: {}", sender_address, error);
                    break;
                }
            };

            // Zero bytes means the sender closed its connection.
            if bytes_read == 0 {
                break;
            }

            if !message.ends_with('\n') {
                eprintln!(
                    "{} disconnected before finishing a message.",
                    sender_address
                );
                break;
            }

            print!("{}: {}", sender_address, message);
        }

        println!("Disconnected: {}", sender_address);
    }
}
