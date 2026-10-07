// Receive UDP packets; registration and routing live in protocol.rs.
mod protocol;
use protocol::Server;
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

    let mut server = Server::default();
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
        let text = match std::str::from_utf8(&buffer[..count]) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let replies = server.handle(text, source, Instant::now());
        for reply in replies {
            match socket.send_to(reply.text.as_bytes(), reply.address) {
                Ok(_) => {}
                Err(error) => eprintln!("Send failed: {}", error),
            }
        }
    }
}

#[cfg(test)]
mod tests;
