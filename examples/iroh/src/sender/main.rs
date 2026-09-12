// Send one message, wait for the receiver's receipt, then disconnect.

mod arguments;

use std::process::ExitCode;

use bit_to_byte_iroh::network;
use iroh::endpoint::VarInt;
use iroh::{Endpoint, EndpointAddr};

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
    let options = match arguments::read() {
        Ok(Some(options)) => options,
        Ok(None) => return Ok(()),
        Err(error) => return Err(error),
    };

    let endpoint_result = match options.direct_only {
        true => network::open_direct_endpoint(options.local_address).await,
        false => network::open_endpoint(options.relay_only).await,
    };
    let endpoint = match endpoint_result {
        Ok(endpoint) => endpoint,
        Err(error) => return Err(error),
    };

    println!("Connecting to receiver {}...", options.receiver_address.id);

    // Bound the whole exchange, including connection setup and the receipt.
    let result = tokio::time::timeout(
        network::NETWORK_TIMEOUT,
        send_message(&endpoint, options.receiver_address, &options.message),
    )
    .await;

    // Closing the endpoint flushes queued connection shutdown messages.
    endpoint.close().await;

    match result {
        Ok(result) => return result,
        Err(_) => {
            if options.direct_only {
                return Err(String::from(
                    "No receipt within 30 seconds. Check the receiver's IP, port, firewall, and network access. No relay was used. Delivery is unconfirmed.",
                ));
            }
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
