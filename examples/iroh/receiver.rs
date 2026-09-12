// Receive one message at a time and send a receipt back to each sender.

mod network;

use std::env;
use std::net::SocketAddr;
use std::process::ExitCode;

use iroh::endpoint::Incoming;

fn main() -> ExitCode {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.worker_threads(2);
    builder.enable_all();
    let runtime = match builder.build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Could not start the networking runtime: {}", error);
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run()) {
        Ok(()) => return ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error);
            return ExitCode::FAILURE;
        }
    }
}

async fn run() -> Result<(), String> {
    let mut arguments = env::args();
    let _program_name = arguments.next();
    let mut direct_address: Option<SocketAddr> = None;
    match arguments.next() {
        Some(argument) => {
            if argument == "--help" {
                println!("Usage: iroh-receiver [--direct LISTEN_IP:PORT]");
                println!("Leave this running, then copy its sender command to another terminal.");
                return Ok(());
            }
            if argument != "--direct" {
                return Err(String::from(
                    "Unknown receiver option. Use --help for instructions.",
                ));
            }
            let address_text = match arguments.next() {
                Some(text) => text,
                None => {
                    return Err(String::from(
                        "After --direct, enter this computer's IP:PORT.",
                    ));
                }
            };
            let address: SocketAddr = match address_text.parse() {
                Ok(address) => address,
                Err(error) => return Err(format!("Invalid listening IP:PORT: {}", error)),
            };
            if address.ip().is_unspecified() || address.ip().is_multicast() || address.port() == 0 {
                return Err(String::from(
                    "Use a specific local IP and a port greater than zero. For a same-computer test, use 127.0.0.1:47002.",
                ));
            }
            direct_address = Some(address);
        }
        None => {}
    }
    if arguments.next().is_some() {
        return Err(String::from(
            "Too many receiver arguments. Use --help for instructions.",
        ));
    }

    let endpoint_result = match direct_address {
        Some(address) => network::open_direct_endpoint(address).await,
        None => network::open_endpoint(false).await,
    };
    let endpoint = match endpoint_result {
        Ok(endpoint) => endpoint,
        Err(error) => return Err(error),
    };

    let address = endpoint.addr();
    println!();
    println!("Receiver ready. Leave this terminal open. Press Ctrl+C to stop.");
    println!("Run this from the project folder on the other computer:");
    println!();
    match direct_address {
        Some(listen_address) => {
            println!(
                "cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-sender -- --direct {} {} \"Hello from Bit to Byte!\"",
                address.id, listen_address
            );
            println!();
            println!(
                "The receiver firewall must allow UDP to {} from the sending computer.",
                listen_address
            );
            println!(
                "This test accepts messages from any peer that can reach it and knows its ID."
            );
        }
        None => {
            let relay_url = match address.relay_urls().next() {
                Some(url) => url,
                None => {
                    endpoint.close().await;
                    return Err(String::from(
                        "The relay address disappeared. Please restart the receiver.",
                    ));
                }
            };
            println!(
                "cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-sender -- {} {} \"Hello from Bit to Byte!\"",
                address.id, relay_url
            );
        }
    }
    println!();

    loop {
        let incoming = match endpoint.accept().await {
            Some(incoming) => incoming,
            None => {
                endpoint.close().await;
                return Err(String::from("The receiver endpoint closed."));
            }
        };

        // One sender at a time keeps this experiment easy to follow. A stalled
        // sender gets at most 30 seconds before we return to accepting connections.
        match tokio::time::timeout(network::NETWORK_TIMEOUT, receive_message(incoming)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("{}", error),
            Err(_) => eprintln!("The sender took too long. Waiting for another sender."),
        }
    }
}

async fn receive_message(incoming: Incoming) -> Result<(), String> {
    let connection = match incoming.await {
        Ok(connection) => connection,
        Err(error) => return Err(format!("Could not accept the connection: {}", error)),
    };
    println!("Connected: {}", connection.remote_id());

    let (mut send_stream, mut receive_stream) = match connection.accept_bi().await {
        Ok(streams) => streams,
        Err(error) => return Err(format!("Could not accept the message stream: {}", error)),
    };

    // read_to_end waits for finish, even when the message arrives in pieces.
    // The limit prevents a sender from making us buffer an unlimited message.
    let bytes = match receive_stream.read_to_end(network::MAX_MESSAGE_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => return Err(format!("Could not read the message: {}", error)),
    };
    let message = match String::from_utf8(bytes) {
        Ok(message) => message,
        Err(error) => return Err(format!("The message was not valid UTF-8: {}", error)),
    };
    if message.is_empty() {
        return Err(String::from("The sender sent an empty message."));
    }

    // Escape control characters so a message cannot issue terminal commands.
    println!("Received: {}", message.escape_debug());
    network::print_connection_path(&connection);

    match send_stream.write_all(network::RECEIPT).await {
        Ok(()) => {}
        Err(error) => return Err(format!("Could not send the receipt: {}", error)),
    }
    match send_stream.finish() {
        Ok(()) => {}
        Err(error) => return Err(format!("Could not finish the receipt: {}", error)),
    }

    // Keep the connection alive until the sender has read the receipt and closed.
    connection.closed().await;
    println!("Waiting for another sender...");
    return Ok(());
}
