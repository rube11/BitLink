// Receive one message at a time and send a receipt back to each sender.

mod network;

use std::env;
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
    match arguments.next() {
        Some(argument) => {
            if argument == "--help" {
                println!("Usage: iroh-receiver");
                println!("Leave this running, then copy its sender command to another terminal.");
                return Ok(());
            }
            return Err(String::from(
                "The receiver takes no arguments. Use --help for instructions.",
            ));
        }
        None => {}
    }

    let endpoint = match network::open_endpoint(false).await {
        Ok(endpoint) => endpoint,
        Err(error) => return Err(error),
    };

    let address = endpoint.addr();
    let relay_url = match address.relay_urls().next() {
        Some(url) => url,
        None => {
            endpoint.close().await;
            return Err(String::from(
                "The relay address disappeared. Please restart the receiver.",
            ));
        }
    };

    println!();
    println!("Receiver ready. Leave this terminal open. Press Ctrl+C to stop.");
    println!("Run this from the project folder on the other computer:");
    println!();
    println!(
        "cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-sender -- {} {} \"Hello from Bit to Byte!\"",
        address.id, relay_url
    );
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
