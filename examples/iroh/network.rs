// The connection setup and protocol values shared by the two programs.

use std::time::Duration;

use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, RelayMode};

// Both programs must agree on the protocol before iroh accepts a connection.
pub const PROTOCOL: &[u8] = b"bit-to-byte/connection-test/1";
pub const MAX_MESSAGE_BYTES: usize = 4096;
pub const RECEIPT: &[u8] = b"received";
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn open_endpoint(relay_only: bool) -> Result<Endpoint, String> {
    // The sender command includes the receiver's relay, so DNS peer lookup is unnecessary.
    let mut builder = Endpoint::builder(presets::Minimal);
    builder = builder.relay_mode(RelayMode::Default);
    builder = builder.alpns(vec![PROTOCOL.to_vec()]);

    if relay_only {
        // A diagnostic mode: do not let a same-computer test succeed over loopback.
        builder = builder.clear_ip_transports();
        println!("Relay-only mode: direct connections are disabled for this test.");
    }

    let endpoint = match builder.bind().await {
        Ok(endpoint) => endpoint,
        Err(error) => {
            return Err(format!("Could not open the network endpoint: {}", error));
        }
    };

    println!("Connecting to a public iroh relay (up to 30 seconds)...");
    match tokio::time::timeout(NETWORK_TIMEOUT, endpoint.online()).await {
        Ok(()) => {
            return Ok(endpoint);
        }
        Err(_) => {
            endpoint.close().await;
            return Err(String::from(
                "Could not reach a public relay within 30 seconds. Check internet access and network restrictions, then try again.",
            ));
        }
    }
}

pub fn print_connection_path(connection: &Connection) {
    // This is a snapshot. Iroh may switch from a relay to a direct path later.
    let paths = connection.paths();
    for path in paths.iter() {
        if path.is_selected() {
            if path.is_ip() {
                println!("Current connection path: direct.");
            } else if path.is_relay() {
                println!("Current connection path: relay.");
            } else {
                println!("Current connection path: another transport.");
            }
            return;
        }
    }

    println!("Current connection path: not available.");
}
