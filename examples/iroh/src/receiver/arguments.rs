// Read and validate receiver options before opening sockets or requesting permissions.

use std::env;
use std::net::{IpAddr, SocketAddr};

pub struct Options {
    pub lan: bool,
    pub direct_address: Option<SocketAddr>,
    pub allowed_sender: Option<IpAddr>,
}

// None means --help was displayed and the program should exit successfully.
pub fn read() -> Result<Option<Options>, String> {
    let mut arguments = env::args();
    let _program_name = arguments.next();
    let mut direct_address: Option<SocketAddr> = None;
    let mut allowed_sender: Option<IpAddr> = None;
    match arguments.next() {
        Some(argument) => {
            if argument == "--help" {
                println!("Usage: iroh-receiver [--direct LISTEN_IP:PORT]");
                println!("       iroh-receiver --lan");
                println!("       iroh-receiver --direct LISTEN_IP:PORT --allow-from SENDER_IP");
                println!(
                    "--allow-from offers to add a narrow Ubuntu/UFW firewall rule after confirmation."
                );
                println!("Leave this running, then copy its sender command to another terminal.");
                return Ok(None);
            }
            if argument == "--lan" {
                if arguments.next().is_some() {
                    return Err(String::from(
                        "--lan cannot be combined with other receiver options.",
                    ));
                }
                return Ok(Some(Options {
                    lan: true,
                    direct_address: None,
                    allowed_sender: None,
                }));
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
    match arguments.next() {
        Some(option) => {
            if option != "--allow-from" {
                return Err(String::from(
                    "Expected --allow-from SENDER_IP. Use --help for instructions.",
                ));
            }
            let sender_text = match arguments.next() {
                Some(text) => text,
                None => return Err(String::from("Missing sender IP after --allow-from.")),
            };
            let sender_ip: IpAddr = match sender_text.parse() {
                Ok(ip) => ip,
                Err(error) => return Err(format!("Invalid sender IP: {}", error)),
            };
            if sender_ip.is_unspecified() || sender_ip.is_multicast() {
                return Err(String::from(
                    "Use the sending computer's specific IP address.",
                ));
            }
            match direct_address {
                Some(address) => {
                    if address.is_ipv4() != sender_ip.is_ipv4() {
                        return Err(String::from(
                            "The receiver and sender must use the same IP version.",
                        ));
                    }
                }
                None => {
                    return Err(String::from(
                        "--allow-from requires --direct LISTEN_IP:PORT.",
                    ));
                }
            }
            if arguments.next().is_some() {
                return Err(String::from(
                    "Too many receiver arguments. Use --help for instructions.",
                ));
            }
            allowed_sender = Some(sender_ip);
        }
        None => {}
    }

    return Ok(Some(Options {
        lan: false,
        direct_address: direct_address,
        allowed_sender: allowed_sender,
    }));
}
