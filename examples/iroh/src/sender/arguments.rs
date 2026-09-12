// Read and validate sender options before opening a connection.

use std::env;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use bit_to_byte_iroh::network;
use iroh::{EndpointAddr, EndpointId, RelayUrl};

pub struct Options {
    pub receiver_address: EndpointAddr,
    pub local_address: SocketAddr,
    pub direct_only: bool,
    pub relay_only: bool,
    pub message: String,
}

// None means --help was displayed and the program should exit successfully.
pub fn read() -> Result<Option<Options>, String> {
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
        println!("       iroh-sender --direct RECEIVER_ID RECEIVER_IP:PORT \"MESSAGE\"");
        return Ok(None);
    }

    let mut relay_only = false;
    let mut direct_only = false;
    if receiver_id_text == "--relay-only" {
        relay_only = true;
        receiver_id_text = match arguments.next() {
            Some(text) => text,
            None => return Err(String::from("Missing receiver ID after --relay-only.")),
        };
    } else if receiver_id_text == "--direct" {
        direct_only = true;
        receiver_id_text = match arguments.next() {
            Some(text) => text,
            None => return Err(String::from("Missing receiver ID after --direct.")),
        };
    }

    let receiver_id: EndpointId = match receiver_id_text.parse() {
        Ok(id) => id,
        Err(error) => return Err(format!("Invalid receiver ID: {}", error)),
    };

    let address_text = match arguments.next() {
        Some(text) => text,
        None => {
            return Err(String::from(
                "Missing receiver address. Copy the command printed by the receiver.",
            ));
        }
    };
    let mut receiver_address = EndpointAddr::new(receiver_id);
    let mut local_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    if direct_only {
        let address: SocketAddr = match address_text.parse() {
            Ok(address) => address,
            Err(error) => return Err(format!("Invalid receiver IP:PORT: {}", error)),
        };
        if address.ip().is_unspecified() || address.ip().is_multicast() || address.port() == 0 {
            return Err(String::from(
                "Use the receiver's specific IP and a port greater than zero.",
            ));
        }
        if address.is_ipv6() {
            local_address = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0);
        }
        receiver_address = receiver_address.with_ip_addr(address);
    } else {
        let relay_url: RelayUrl = match address_text.parse() {
            Ok(url) => url,
            Err(error) => return Err(format!("Invalid relay URL: {}", error)),
        };
        receiver_address = receiver_address.with_relay_url(relay_url);
    }

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

    return Ok(Some(Options {
        receiver_address: receiver_address,
        local_address: local_address,
        direct_only: direct_only,
        relay_only: relay_only,
        message: message,
    }));
}
