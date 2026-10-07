// Chat and sharing use isolated registrations on the same UDP relay.
mod protocol;
use protocol::Relay;

#[cfg(test)]
use protocol::*;
#[cfg(test)]
use std::net::SocketAddr;
use std::{io, net::UdpSocket, time::Instant};

fn main() -> io::Result<()> {
    let address = match std::env::args().nth(1) {
        Some(address) => address,
        None => String::from("0.0.0.0:47002"),
    };
    let socket = match UdpSocket::bind(&address) {
        Ok(socket) => socket,
        Err(error) => return Err(error),
    };
    println!("Presence and chat relay listening on {}", address);

    let mut server = Relay::default();
    let mut buffer = [0_u8; 4096];
    loop {
        let (count, source) = match socket.recv_from(&mut buffer) {
            Ok(packet) => packet,
            Err(error) => {
                eprintln!("Receive failed: {}", error);
                continue;
            }
        };
        // A full buffer may mean that the packet was cut short.
        if count == buffer.len() {
            continue;
        }
        for (address, packet) in server.handle(&buffer[..count], source, Instant::now()) {
            match socket.send_to(&packet, address) {
                Ok(_) => {}
                Err(error) => eprintln!("Send failed: {}", error),
            }
        }
    }
}

#[cfg(test)]
mod tests;
