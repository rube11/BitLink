// Socket setup. The discovery loop lives in this folder's mod.rs.

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};

use socket2::{Domain, Protocol, SockAddr, SockRef, Socket, Type};

use super::{DISCOVERY_GROUP, DISCOVERY_PORT};

pub fn open_discovery_socket() -> io::Result<UdpSocket> {
    let socket = match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
        Ok(socket) => socket,
        Err(error) => return Err(error),
    };

    // Share the discovery port so multiple app windows work on one computer.
    match socket.set_reuse_address(true) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    #[cfg(unix)]
    match socket.set_reuse_port(true) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    let bind_address = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT);
    let socket_address = SockAddr::from(bind_address);
    match socket.bind(&socket_address) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }

    // Listen on the network and on loopback (this computer).
    match socket.join_multicast_v4(&DISCOVERY_GROUP, &Ipv4Addr::UNSPECIFIED) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    match socket.join_multicast_v4(&DISCOVERY_GROUP, &Ipv4Addr::LOCALHOST) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    match socket.set_multicast_loop_v4(true) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    // Keep discovery traffic on the local network.
    match socket.set_multicast_ttl_v4(1) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    // Return immediately when there is no packet, so the TUI can handle input.
    match socket.set_nonblocking(true) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    return Ok(UdpSocket::from(socket));
}

pub fn open_local_sender() -> io::Result<UdpSocket> {
    let socket = match UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)) {
        Ok(socket) => socket,
        Err(error) => return Err(error),
    };
    // SockRef configures this existing socket; it does not open another one.
    let options = SockRef::from(&socket);
    match options.set_multicast_if_v4(&Ipv4Addr::LOCALHOST) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    match socket.set_nonblocking(true) {
        Ok(()) => {}
        Err(error) => return Err(error),
    }
    return Ok(socket);
}
