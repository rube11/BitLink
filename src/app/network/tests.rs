use super::*;

fn test_network() -> (UdpSocket, Network) {
    let fake_server = match UdpSocket::bind("127.0.0.1:0") {
        Ok(socket) => socket,
        Err(error) => panic!("Could not bind the fake server: {}", error),
    };
    match fake_server.set_read_timeout(Some(Duration::from_secs(1))) {
        Ok(()) => {}
        Err(error) => panic!("Could not set the server timeout: {}", error),
    }
    let server_address = match fake_server.local_addr() {
        Ok(address) => address.to_string(),
        Err(error) => panic!("Could not read the fake server address: {}", error),
    };
    let network = match Network::connect_to(Some(String::from("Alice")), &server_address) {
        Ok(network) => network,
        Err(error) => panic!("Could not connect: {}", error),
    };
    return (fake_server, network);
}

fn pending_message(
    person_id: &str,
    text: &str,
    index: usize,
    token: &str,
    first_sent: Instant,
) -> PendingMessage {
    return PendingMessage {
        outgoing: OutgoingMessage {
            person_id: String::from(person_id),
            text: String::from(text),
            message_index: index,
        },
        token: String::from(token),
        first_sent: first_sent,
        next_send: first_sent,
    };
}

#[test]
fn default_name_uses_the_random_id() {
    let (_fake_server, network) = test_network();
    assert_eq!(network.name(), "Alice");
    assert_eq!(network.id.len(), 24);
    let unnamed = match Network::connect_to(None, "127.0.0.1:1") {
        Ok(network) => network,
        Err(error) => panic!("Could not connect: {}", error),
    };
    assert!(unnamed.name().starts_with("Member "));
    assert_eq!(unnamed.name().len(), 13);

    // An explicitly chosen name is never treated as a default-name marker.
    let named = match Network::connect_to(Some(String::from("Club member")), "127.0.0.1:1") {
        Ok(network) => network,
        Err(error) => panic!("Could not connect: {}", error),
    };
    assert_eq!(named.name(), "Club member");
    assert!(Network::connect_to(Some(String::from("   ")), "127.0.0.1:1").is_err());
}

fn read_packet(socket: &UdpSocket) -> (String, SocketAddr) {
    let mut buffer = [0_u8; 4096];
    let (count, source) = match socket.recv_from(&mut buffer) {
        Ok(packet) => packet,
        Err(error) => panic!("Did not receive the expected packet: {}", error),
    };
    let text = match std::str::from_utf8(&buffer[..count]) {
        Ok(text) => String::from(text),
        Err(error) => panic!("Received invalid text: {}", error),
    };
    return (text, source);
}

fn send_packet(socket: &UdpSocket, text: &str, address: SocketAddr) {
    match socket.send_to(text.as_bytes(), address) {
        Ok(_) => {}
        Err(error) => panic!("Could not send a test packet: {}", error),
    }
}

#[test]
fn socket_traffic_covers_registration_retries_receipts_and_goodbye() {
    let (server, mut network) = test_network();
    let mut app = App::new();
    network.update(&mut app);
    let (registration, client_address) = read_packet(&server);
    assert_eq!(registration, format!("REGISTER {} Alice", network.id));
    send_packet(&server, "REGISTERED 127.0.0.1:1234", client_address);
    send_packet(&server, "bit-to-byte/1\nhello\nbob\nBob", client_address);
    match server.send_to(&[255], client_address) {
        Ok(_) => {}
        Err(error) => panic!("Could not send invalid UTF-8: {}", error),
    }
    network.update(&mut app);
    assert_eq!(app.network_status, "Relay connected");
    assert_eq!(app.people.len(), 1);
    assert!(app.people[0].online);

    app.people[0].messages.push(Message::sent("hello 界"));
    app.outbox.push(OutgoingMessage {
        person_id: String::from("bob"),
        text: String::from("hello 界"),
        message_index: 0,
    });
    network.update(&mut app);
    let (first_send, _) = read_packet(&server);
    let token = format!("{}-1", network.id);
    assert_eq!(first_send, format!("RELAY bob {} CHAT hello 界", token));
    assert!(app.outbox.is_empty());

    // Advance the retry clock without making the test sleep.
    let retry_time = Instant::now() + RESEND_EVERY;
    network.resend_pending_messages(&mut app, retry_time);
    let (retry, _) = read_packet(&server);
    assert_eq!(retry, first_send);
    send_packet(
        &server,
        &format!("FROM bob {} RECEIPT", token),
        client_address,
    );
    network.receive_packets(&mut app, retry_time);
    assert!(network.pending.is_empty());
    assert_eq!(
        app.people[0].messages[0].delivery,
        Some(Delivery::Delivered)
    );

    for _repeat in 0..2 {
        send_packet(&server, "FROM bob reply-1 CHAT You: hello", client_address);
        network.receive_packets(&mut app, retry_time);
        let (receipt, _) = read_packet(&server);
        assert_eq!(receipt, "RELAY bob reply-1 RECEIPT");
    }
    assert_eq!(app.people[0].messages.len(), 2);
    assert_eq!(app.people[0].messages[1], Message::received("You: hello"));

    // A silent relay changes the status, but keeps conversations and drafts.
    app.people[0].draft = String::from("keep this");
    let later = retry_time + RELAY_SILENT_AFTER;
    network.update_status(&mut app, later);
    presence::mark_missing_people_offline(&mut app, later);
    assert_eq!(app.network_status, "Relay unavailable · retrying");
    assert!(!app.people[0].online);
    assert_eq!(app.people[0].draft, "keep this");
    assert_eq!(app.people[0].messages.len(), 2);
    network.goodbye();
    let (goodbye, _) = read_packet(&server);
    assert_eq!(goodbye, format!("GOODBYE {}", network.id));
}

#[test]
fn receipts_are_matched_and_repeated_messages_are_shown_once() {
    let (_fake_server, mut network) = test_network();
    let mut app = App::new();
    let now = Instant::now();

    network.handle_packet(&mut app, "bit-to-byte/1\nhello\nbob\nBob", now);
    network.handle_packet(&mut app, "FROM bob test CHAT hello", now);
    network.handle_packet(&mut app, "FROM bob test CHAT hello", now);
    assert_eq!(app.people[0].messages, vec![Message::received("hello")]);

    app.people[0].messages.push(Message::sent("reply"));
    network
        .pending
        .push(pending_message("bob", "reply", 1, "expected", now));
    network.handle_packet(&mut app, "FROM bob wrong RECEIPT", now);
    network.handle_packet(&mut app, "FROM stranger expected RECEIPT", now);
    assert_eq!(network.pending.len(), 1);
    network.handle_packet(&mut app, "FROM bob expected RECEIPT", now);
    assert!(network.pending.is_empty());
    assert_eq!(
        app.people[0].messages[1].delivery,
        Some(Delivery::Delivered)
    );

    network.handle_packet(&mut app, "bit-to-byte/1\ngoodbye\nbob\nBob", now);
    assert!(!app.people[0].online);
}

#[test]
fn other_sources_are_ignored_and_unanswered_messages_become_unconfirmed() {
    let (_fake_server, mut network) = test_network();
    let mut app = App::new();

    let attacker = match UdpSocket::bind("127.0.0.1:0") {
        Ok(socket) => socket,
        Err(error) => panic!("Could not bind the attacker socket: {}", error),
    };
    let our_address = match network.socket.local_addr() {
        Ok(address) => address,
        Err(error) => panic!("Could not read our address: {}", error),
    };
    match attacker.send_to(b"bit-to-byte/1\nhello\nevil\nEvil", our_address) {
        Ok(_) => {}
        Err(error) => panic!("Could not send the attacker packet: {}", error),
    }
    network.update(&mut app);
    assert!(app.people.is_empty());

    let now = Instant::now();
    network.handle_packet(&mut app, "bit-to-byte/1\nhello\nbob\nBob", now);
    app.people[0].messages.push(Message::sent("hello"));
    let long_ago = now - GIVE_UP_AFTER - Duration::from_secs(1);
    network
        .pending
        .push(pending_message("bob", "hello", 0, "expected", long_ago));
    network.update(&mut app);
    assert!(network.pending.is_empty());
    assert_eq!(
        app.people[0].messages[0].delivery,
        Some(Delivery::Unconfirmed)
    );
}
