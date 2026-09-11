// Send one message, wait for the receiver's receipt, then disconnect.

mod network;

use std::env;
use std::process::ExitCode;

use iroh::endpoint::VarInt;
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayUrl};

fn main() -> ExitCode {
    // Tokio runs iroh's asynchronous network work while our code waits with .await.
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
    let mut receiver_id_text = match arguments.next() {
        Some(text) => text,
        None => {
            return Err(String::from(
                "Copy the sender command printed by the receiver.",
            ));
        }
    };

    if receiver_id_text == "--help" {
        println!("Usage: iroh-sender [--relay-only] RECEIVER_ID RELAY_URL \"MESSAGE\"");
        return Ok(());
    }

    let mut relay_only = false;
    if receiver_id_text == "--relay-only" {
        relay_only = true;
        receiver_id_text = match arguments.next() {
            Some(text) => text,
            None => return Err(String::from("Missing receiver ID after --relay-only.")),
        };
    }

    let receiver_id: EndpointId = match receiver_id_text.parse() {
        Ok(id) => id,
        Err(error) => return Err(format!("Invalid receiver ID: {}", error)),
    };

    let relay_url_text = match arguments.next() {
        Some(text) => text,
        None => {
            return Err(String::from(
                "Missing relay URL. Copy it from the receiver.",
            ));
        }
    };
    let relay_url: RelayUrl = match relay_url_text.parse() {
        Ok(url) => url,
        Err(error) => return Err(format!("Invalid relay URL: {}", error)),
    };

    let message = match arguments.next() {
        Some(message) => message,
        None => return Err(String::from("Missing message. Put your message in quotes.")),
    };

    if arguments.next().is_some() {
        return Err(String::from(
            "Too many arguments. Put your entire message in quotes.",
        ));
    }
    if message.is_empty() || message.len() > network::MAX_MESSAGE_BYTES {
        return Err(String::from(
            "The message must contain between 1 and 4096 UTF-8 bytes.",
        ));
    }

    let endpoint = match network::open_endpoint(relay_only).await {
        Ok(endpoint) => endpoint,
        Err(error) => return Err(error),
    };

    let mut receiver_address = EndpointAddr::new(receiver_id);
    receiver_address = receiver_address.with_relay_url(relay_url);
    println!("Connecting to receiver {}...", receiver_id);

    // Bound the whole exchange, including connection setup and the receipt.
    let result = tokio::time::timeout(
        network::NETWORK_TIMEOUT,
        send_message(&endpoint, receiver_address, &message),
    )
    .await;

    // Closing the endpoint flushes queued connection shutdown messages.
    endpoint.close().await;

    match result {
        Ok(result) => return result,
        Err(_) => {
            return Err(String::from(
                "No receipt within 30 seconds. Keep the receiver running and check its ID and relay URL. Delivery is unconfirmed.",
            ));
        }
    }
}

async fn send_message(
    endpoint: &Endpoint,
    receiver_address: EndpointAddr,
    message: &str,
) -> Result<(), String> {
    let connection = match endpoint.connect(receiver_address, network::PROTOCOL).await {
        Ok(connection) => connection,
        Err(error) => return Err(format!("Could not connect to the receiver: {}", error)),
    };

    // A bidirectional stream lets us send the message and receive a receipt.
    let (mut send_stream, mut receive_stream) = match connection.open_bi().await {
        Ok(streams) => streams,
        Err(error) => return Err(format!("Could not open a message stream: {}", error)),
    };

    match send_stream.write_all(message.as_bytes()).await {
        Ok(()) => {}
        Err(error) => return Err(format!("Could not send the message: {}", error)),
    }

    // finish marks the end of this message. It does not close the connection.
    match send_stream.finish() {
        Ok(()) => {}
        Err(error) => return Err(format!("Could not finish the message: {}", error)),
    }

    let receipt = match receive_stream.read_to_end(network::RECEIPT.len()).await {
        Ok(receipt) => receipt,
        Err(error) => return Err(format!("Could not read the receipt: {}", error)),
    };
    if receipt != network::RECEIPT {
        return Err(String::from("The receiver sent an unexpected receipt."));
    }

    println!("Delivered: the receiver confirmed your message.");
    network::print_connection_path(&connection);
    connection.close(VarInt::from_u32(0), b"message received");
    return Ok(());
}
