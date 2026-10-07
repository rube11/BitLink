use super::*;

mod direct;
mod global;

const TEST_KEY: [u8; 32] = [7_u8; 32];

fn incoming(network: &Network, sender: &str, payload: &str) -> String {
    let encrypted =
        crypto::encrypt(&TEST_KEY, sender, &network.id, payload).expect("encrypt packet");
    return format!("FROM {} ENC1 {}", sender, encrypted);
}

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
    let network = match Network::connect(Some(String::from("Alice")), &server_address, TEST_KEY) {
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
            global: false,
        },
        token: String::from(token),
        encrypted_payload: String::new(), // These fixtures only test receipts and timeouts.
        first_sent: first_sent,
        next_send: first_sent,
    };
}

#[test]
fn default_name_uses_the_random_id() {
    let (_fake_server, network) = test_network();
    assert_eq!(network.name, "Alice");
    assert_eq!(network.id.len(), 24);
    let unnamed = match Network::connect(None, "127.0.0.1:1", TEST_KEY) {
        Ok(network) => network,
        Err(error) => panic!("Could not connect: {}", error),
    };
    assert!(unnamed.name.starts_with("Member "));
    assert_eq!(unnamed.name.len(), 13);

    // An explicitly chosen name is never treated as a default-name marker.
    let named = match Network::connect(Some(String::from("Club member")), "127.0.0.1:1", TEST_KEY) {
        Ok(network) => network,
        Err(error) => panic!("Could not connect: {}", error),
    };
    assert_eq!(named.name, "Club member");
    assert!(Network::connect(Some(String::from("   ")), "127.0.0.1:1", TEST_KEY).is_err());
}

#[test]
fn registration_errors_remain_visible_until_registration_succeeds() {
    let (server, mut network) = test_network();
    let mut app = App::new();
    let now = Instant::now();
    network.update(&mut app, now);
    let (_, address) = read_packet(&server);
    for (error, status) in [
        ("ERROR address-in-use", "Reconnect blocked · retrying"),
        ("ERROR id-in-use", "Session ID in use · retrying"),
    ] {
        send_packet(&server, "REGISTERED endpoint", address);
        network.update(&mut app, now);
        assert_eq!(app.network_status, "Relay connected");
        send_packet(&server, error, address);
        network.update(&mut app, now);
        assert_eq!(app.network_status, status);
        network.update(&mut app, now + Duration::from_millis(200));
        assert_eq!(app.network_status, status);
    }
    send_packet(&server, "REGISTERED endpoint", address);
    network.update(&mut app, now + Duration::from_millis(400));
    assert_eq!(app.network_status, "Relay connected");
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
    network.update(&mut app, Instant::now());
    let (registration, client_address) = read_packet(&server);
    assert_eq!(registration, format!("REGISTER {} Alice", network.id));
    send_packet(&server, "REGISTERED 127.0.0.1:1234", client_address);
    send_packet(&server, "bit-to-byte/1\nhello\nbob\nBob", client_address);
    match server.send_to(&[255], client_address) {
        Ok(_) => {}
        Err(error) => panic!("Could not send invalid UTF-8: {}", error),
    }
    network.update(&mut app, Instant::now());
    assert_eq!(app.network_status, "Relay connected");
    assert_eq!(app.people.len(), 1);
    assert_eq!(read_packet(&server).0, "DISCOVER bob");

    app.people[0].messages.push(Message::sent("hello 界"));
    app.outbox.push(OutgoingMessage {
        person_id: String::from("bob"),
        text: String::from("hello 界"),
        message_index: 0,
        global: false,
    });
    network.update(&mut app, Instant::now());
    let (first_send, _) = read_packet(&server);
    let token = format!("{}-1", network.id);
    assert!(first_send.starts_with("RELAY bob ENC1 "));
    assert!(!first_send.contains("hello"));
    let encrypted = first_send.trim_start_matches("RELAY bob ENC1 ");
    assert_eq!(
        crypto::decrypt(&TEST_KEY, &network.id, "bob", encrypted).expect("decrypt message"),
        format!("{} CHAT hello 界", token)
    );
    assert!(app.outbox.is_empty());

    // Advance the retry clock without making the test sleep.
    let retry_time = Instant::now() + RESEND_EVERY;
    network.resend_pending_messages(&mut app, retry_time);
    let (retry, _) = read_packet(&server);
    assert_eq!(retry, first_send);
    send_packet(
        &server,
        &incoming(&network, "bob", &format!("{} RECEIPT", token)),
        client_address,
    );
    network.receive_packets(&mut app, retry_time);
    assert!(network.pending.is_empty());
    assert_eq!(
        app.people[0].messages[0].delivery,
        Some(Delivery::Delivered)
    );

    for _repeat in 0..2 {
        send_packet(
            &server,
            &incoming(&network, "bob", "reply-1 CHAT You: hello"),
            client_address,
        );
        network.receive_packets(&mut app, retry_time);
        let (receipt, _) = read_packet(&server);
        assert!(receipt.starts_with("RELAY bob ENC1 "));
        let encrypted = receipt.trim_start_matches("RELAY bob ENC1 ");
        assert_eq!(
            crypto::decrypt(&TEST_KEY, &network.id, "bob", encrypted).expect("decrypt receipt"),
            "reply-1 RECEIPT"
        );
    }
    assert_eq!(app.people[0].messages.len(), 2);
    assert_eq!(app.people[0].messages[1], Message::received("You: hello"));

    // A silent relay changes the status; missing peers leave the people list.
    let later = retry_time + RELAY_SILENT_AFTER;
    network.last_server_reply = Some(Instant::now() - RELAY_SILENT_AFTER);
    network.update(&mut app, Instant::now());
    presence::remove_missing_people(&mut app, later);
    assert_eq!(app.network_status, "Relay unavailable · retrying");
    assert!(app.people.is_empty());
    network.goodbye();
    let (goodbye, _) = read_packet(&server);
    assert_eq!(goodbye, format!("GOODBYE {}", network.id));
}

#[test]
fn receipts_are_matched_and_repeated_messages_are_shown_once() {
    let (_fake_server, mut network) = test_network();
    let mut app = App::new();
    let now = Instant::now();

    network.handle_packet(&mut app, "bit-to-byte/1\nhello\nbob\nBob", None, now);
    let packet = incoming(&network, "bob", "test CHAT hello");
    network.handle_packet(&mut app, &packet, None, now);
    let packet = incoming(&network, "bob", "test CHAT hello");
    network.handle_packet(&mut app, &packet, None, now);
    assert_eq!(app.people[0].messages, vec![Message::received("hello")]);

    app.people[0].messages.push(Message::sent("reply"));
    network
        .pending
        .push(pending_message("bob", "reply", 1, "expected", now));
    let packet = incoming(&network, "bob", "wrong RECEIPT");
    network.handle_packet(&mut app, &packet, None, now);
    let packet = incoming(&network, "stranger", "expected RECEIPT");
    network.handle_packet(&mut app, &packet, None, now);
    assert_eq!(network.pending.len(), 1);
    let packet = incoming(&network, "bob", "expected RECEIPT");
    network.handle_packet(&mut app, &packet, None, now);
    assert!(network.pending.is_empty());
    assert_eq!(
        app.people[0].messages[1].delivery,
        Some(Delivery::Delivered)
    );

    network.handle_packet(&mut app, "bit-to-byte/1\ngoodbye\nbob\nBob", None, now);
    assert!(app.people.is_empty());
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
    network.update(&mut app, Instant::now());
    assert!(app.people.is_empty());

    let now = Instant::now();
    network.handle_packet(&mut app, "bit-to-byte/1\nhello\nbob\nBob", None, now);
    app.people[0].messages.push(Message::sent("hello"));
    let long_ago = now - GIVE_UP_AFTER - Duration::from_secs(1);
    network
        .pending
        .push(pending_message("bob", "hello", 0, "expected", long_ago));
    network.update(&mut app, Instant::now());
    assert!(network.pending.is_empty());
    assert_eq!(
        app.people[0].messages[0].delivery,
        Some(Delivery::Unconfirmed)
    );
}

#[test]
fn plaintext_and_wrong_key_packets_cannot_deliver_chat_or_confirm_messages() {
    let (server, mut network) = test_network();
    let mut app = App::new();
    let now = Instant::now();
    network.handle_packet(&mut app, "bit-to-byte/1\nhello\nbob\nBob", None, now);
    app.people[0].messages.push(Message::sent("reply"));
    network
        .pending
        .push(pending_message("bob", "reply", 0, "expected", now));

    for payload in ["test CHAT hello", "test GLOBAL hello", "expected RECEIPT"] {
        network.handle_packet(&mut app, &format!("FROM bob {}", payload), None, now);
        let encrypted = crypto::encrypt(&[8_u8; 32], "bob", &network.id, payload).expect("encrypt");
        network.handle_packet(&mut app, &format!("FROM bob ENC1 {}", encrypted), None, now);
    }
    assert_eq!(app.people[0].messages, vec![Message::sent("reply")]);
    assert_eq!(network.pending.len(), 1);
    assert!(app.global_messages.is_empty());
    // Invalid chat must not produce a receipt.
    server.set_nonblocking(true).expect("nonblocking");
    let mut buffer = [0_u8; 4096];
    let error = server
        .recv_from(&mut buffer)
        .expect_err("no receipt for rejected chat");
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
}

#[test]
fn largest_allowed_unicode_message_fits_the_existing_relay() {
    let (_server, network) = test_network();
    let message = "😀".repeat(MAX_MESSAGE_BYTES / 4);
    let token = format!("{}-{}", network.id, u64::MAX);
    let payload = format!("{} CHAT {}", token, message);
    let packet = network
        .encrypted_payload("bob", &payload)
        .expect("packet fits relay");
    let relay_payload = packet.as_str();
    assert!(relay_payload.len() <= 2100);
    network
        .encrypted_payload("bob", &format!("{token} GLOBAL {message}"))
        .expect("global payload fits relay");
    let decrypted = crypto::decrypt(
        &TEST_KEY,
        &network.id,
        "bob",
        relay_payload.trim_start_matches("ENC1 "),
    )
    .expect("decrypt");
    assert_eq!(decrypted, payload);
    assert!(
        network
            .encrypted_payload("bob", &"😀".repeat(1000))
            .is_err()
    );
}
