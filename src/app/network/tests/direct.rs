use super::*;

fn client_address(network: &Network) -> SocketAddr {
    let port = network.socket.local_addr().expect("client address").port();
    return SocketAddr::from(([127, 0, 0, 1], port));
}

fn assert_no_packet(socket: &UdpSocket) {
    socket.set_nonblocking(true).expect("nonblocking");
    let mut buffer = [0_u8; 4096];
    let error = socket
        .recv_from(&mut buffer)
        .expect_err("no packet expected");
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    socket.set_nonblocking(false).expect("blocking");
}

fn queue_chat(network: &mut Network, app: &mut App, recipient: &str, text: &str, now: Instant) {
    let person = app.find_person(recipient).expect("known recipient");
    let index = person.messages.len();
    person.messages.push(Message::sent(text));
    app.outbox.push(OutgoingMessage {
        person_id: recipient.to_string(),
        text: text.to_string(),
        message_index: index,
    });
    network.queue_outgoing_messages(app, now);
    network.resend_pending_messages(app, now);
}

#[test]
fn introduced_clients_exchange_direct_chat_then_fall_back_without_duplicates() {
    let (server, mut alice) = test_network();
    let mut bob = Network::connect(
        Some(String::from("Bob Smith")),
        &server.local_addr().expect("server address").to_string(),
        TEST_KEY,
    )
    .expect("connect Bob");
    let mut alice_app = App::new();
    let mut bob_app = App::new();
    let alice_address = client_address(&alice);
    let bob_address = client_address(&bob);
    let now = Instant::now();
    alice.next_register = now + Duration::from_secs(60);
    bob.next_register = now + Duration::from_secs(60);

    // Introductions work without separate presence packets, including names
    // with spaces, and probes leave the same socket used for registration.
    send_packet(
        &server,
        &format!("PEER {} {} Bob Smith", bob.id, bob_address),
        alice_address,
    );
    send_packet(
        &server,
        &format!("PEER {} {} Alice", alice.id, alice_address),
        bob_address,
    );
    alice.receive_packets(&mut alice_app, now);
    bob.receive_packets(&mut bob_app, now);
    assert_eq!(alice_app.people[0].name, "Bob Smith");
    alice.update(&mut alice_app, now);
    bob.update(&mut bob_app, now);
    assert_eq!(
        read_packet(&server),
        (format!("DISCOVER {}", bob.id), alice_address)
    );
    assert_eq!(
        read_packet(&server),
        (format!("DISCOVER {}", alice.id), bob_address)
    );
    alice.receive_packets(&mut alice_app, now);
    bob.receive_packets(&mut bob_app, now);
    alice.receive_packets(&mut alice_app, now);
    assert_eq!(alice.direct_address(&bob.id, now), Some(bob_address));
    assert_eq!(bob.direct_address(&alice.id, now), Some(alice_address));
    assert_no_packet(&server);

    queue_chat(&mut alice, &mut alice_app, &bob.id, "direct hello 界", now);
    assert_eq!(alice_app.people[0].messages[0].route, Some("direct UDP"));
    bob.receive_packets(&mut bob_app, now);
    alice.receive_packets(&mut alice_app, now);
    assert_eq!(
        bob_app.people[0].messages,
        vec![Message::received("direct hello 界")]
    );
    assert!(alice.pending.is_empty());
    assert_eq!(
        alice_app.people[0].messages[0].delivery,
        Some(Delivery::Delivered)
    );
    assert_no_packet(&server);

    // Lose a receipt while probes still consider both routes healthy.
    queue_chat(&mut alice, &mut alice_app, &bob.id, "lost receipt", now);
    assert_eq!(alice_app.people[0].messages[1].route, Some("direct UDP"));
    let ciphertext = alice.pending[0].encrypted_payload.clone();
    bob.receive_packets(&mut bob_app, now);
    let (lost_receipt, source) = read_packet(&alice.socket);
    assert_eq!(source, bob_address);
    assert!(lost_receipt.starts_with(&format!("FROM {} ENC1 ", bob.id)));

    let fallback_time = now + DIRECT_RETRY_FOR;
    alice.resend_pending_messages(&mut alice_app, fallback_time);
    assert_eq!(alice_app.people[0].messages[1].route, Some("relay"));
    let (relayed, source) = read_packet(&server);
    assert_eq!(source, alice_address);
    assert_eq!(relayed, format!("RELAY {} {}", bob.id, ciphertext));
    send_packet(
        &server,
        &format!("FROM {} {}", alice.id, ciphertext),
        bob_address,
    );
    bob.receive_packets(&mut bob_app, fallback_time);
    // A relayed retry must get a relayed receipt even with a healthy direct
    // route, so failure of that route cannot defeat fallback.
    let (receipt, source) = read_packet(&server);
    assert_eq!(source, bob_address);
    let payload = receipt
        .strip_prefix(&format!("RELAY {} ", alice.id))
        .expect("relayed receipt");
    send_packet(
        &server,
        &format!("FROM {} {}", bob.id, payload),
        alice_address,
    );
    alice.receive_packets(&mut alice_app, fallback_time);
    assert!(alice.pending.is_empty());
    assert_eq!(
        alice_app.people[0].messages[1].delivery,
        Some(Delivery::Delivered)
    );
    assert_eq!(bob_app.people[0].messages.len(), 2);

    // Without fresh probe replies, even a new message starts on the relay.
    let stale = now + Duration::from_secs(6);
    queue_chat(&mut alice, &mut alice_app, &bob.id, "stale route", stale);
    assert_eq!(alice_app.people[0].messages[2].route, Some("relay"));
    assert!(
        read_packet(&server)
            .0
            .starts_with(&format!("RELAY {} ENC1 ", bob.id))
    );
    assert_no_packet(&bob.socket);
}

#[test]
fn direct_handshakes_require_encryption_the_expected_source_and_a_current_challenge() {
    let (server, mut network) = test_network();
    let peer = UdpSocket::bind("127.0.0.1:0").expect("peer socket");
    peer.set_read_timeout(Some(Duration::from_secs(1)))
        .expect("timeout");
    let attacker = UdpSocket::bind("127.0.0.1:0").expect("attacker socket");
    let peer_address = peer.local_addr().expect("peer address");
    let our_address = client_address(&network);
    let now = Instant::now();
    network.next_register = now + Duration::from_secs(60);
    let mut app = App::new();
    let introduction = format!("PEER bob {} Bob", peer_address);
    send_packet(&attacker, &introduction, our_address);
    network.receive_packets(&mut app, now);
    assert!(app.people.is_empty());
    send_packet(&server, &introduction, our_address);
    network.receive_packets(&mut app, now);
    network.update(&mut app, now);
    assert_eq!(read_packet(&server).0, "DISCOVER bob");
    let (probe, source) = read_packet(&peer);
    assert_eq!(source, our_address);
    let encrypted = probe
        .strip_prefix(&format!("FROM {} ENC1 ", network.id))
        .expect("encrypted probe");
    let plaintext =
        crypto::decrypt(&TEST_KEY, &network.id, "bob", encrypted).expect("decrypt probe");
    let token = plaintext.strip_suffix(" PUNCH").expect("challenge");
    let valid_ack = incoming(&network, "bob", &format!("{} PUNCH_ACK", token));
    send_packet(&attacker, &valid_ack, our_address);
    send_packet(&server, &valid_ack, our_address);
    send_packet(&peer, &format!("FROM bob {} PUNCH_ACK", token), our_address);
    let wrong_key = crypto::encrypt(
        &[8_u8; 32],
        "bob",
        &network.id,
        &format!("{} PUNCH_ACK", token),
    )
    .expect("encrypt");
    send_packet(&peer, &format!("FROM bob ENC1 {}", wrong_key), our_address);
    send_packet(
        &peer,
        &incoming(&network, "bob", "wrong PUNCH_ACK"),
        our_address,
    );
    network.receive_packets(&mut app, now);
    assert_eq!(network.direct_address("bob", now), None);
    assert_no_packet(&server);
    assert_no_packet(&peer);

    send_packet(&peer, &valid_ack, our_address);
    network.receive_packets(&mut app, now);
    assert_eq!(network.direct_address("bob", now), Some(peer_address));
    send_packet(&server, &introduction, our_address);
    network.receive_packets(&mut app, now);
    assert_eq!(network.direct_address("bob", now), Some(peer_address));
    let expired = now + Duration::from_secs(6);
    send_packet(&peer, &valid_ack, our_address);
    network.receive_packets(&mut app, expired);
    assert_eq!(network.direct_address("bob", expired), None);
    network.update(&mut app, expired);
    let (probe, _) = read_packet(&peer);
    let encrypted = probe
        .strip_prefix(&format!("FROM {} ENC1 ", network.id))
        .expect("new probe");
    let plaintext =
        crypto::decrypt(&TEST_KEY, &network.id, "bob", encrypted).expect("decrypt probe");
    let token = plaintext.strip_suffix(" PUNCH").expect("new challenge");
    send_packet(&peer, &valid_ack, our_address);
    network.receive_packets(&mut app, expired);
    assert_eq!(
        network.direct_address("bob", expired),
        None,
        "old ACK cannot confirm a new probe"
    );
    send_packet(
        &peer,
        &incoming(&network, "bob", &format!("{} PUNCH_ACK", token)),
        our_address,
    );
    network.receive_packets(&mut app, expired);
    assert_eq!(network.direct_address("bob", expired), Some(peer_address));

    // A relay introduction can replace the endpoint, but cannot confirm it.
    let moved = attacker.local_addr().expect("new endpoint");
    send_packet(&server, &format!("PEER bob {} Bob", moved), our_address);
    network.receive_packets(&mut app, expired);
    assert_eq!(network.direct_address("bob", expired), None);
    send_packet(
        &peer,
        &incoming(&network, "bob", "old-endpoint CHAT ignored"),
        our_address,
    );
    network.receive_packets(&mut app, expired);
    assert!(app.people[0].messages.is_empty());

    send_packet(&peer, "bit-to-byte/1\nhello\nevil\nEvil", our_address);
    send_packet(&peer, "FROM bob plain CHAT injected", our_address);
    network.receive_packets(&mut app, expired);
    assert_eq!(app.people.len(), 1);
    assert!(app.people[0].messages.is_empty());
}

#[test]
fn failed_hole_punching_keeps_encrypted_relay_delivery_available() {
    let (server, mut network) = test_network();
    let peer = UdpSocket::bind("127.0.0.1:0").expect("peer socket");
    peer.set_read_timeout(Some(Duration::from_secs(1)))
        .expect("timeout");
    let now = Instant::now();
    network.next_register = now + Duration::from_secs(60);
    let mut app = App::new();
    network.handle_packet(
        &mut app,
        &format!("PEER bob {} Bob", peer.local_addr().expect("address")),
        None,
        now,
    );
    network.update(&mut app, now);
    assert_eq!(read_packet(&server).0, "DISCOVER bob");
    assert!(read_packet(&peer).0.starts_with("FROM "));
    queue_chat(&mut network, &mut app, "bob", "relay hello", now);
    assert_eq!(app.people[0].messages[0].route, Some("relay"));
    assert!(read_packet(&server).0.starts_with("RELAY bob ENC1 "));
    let receipt = incoming(
        &network,
        "bob",
        &format!("{} RECEIPT", network.pending[0].token),
    );
    send_packet(&server, &receipt, client_address(&network));
    network.receive_packets(&mut app, now);
    assert!(network.pending.is_empty());
    assert_eq!(
        app.people[0].messages[0].delivery,
        Some(Delivery::Delivered)
    );
    assert_no_packet(&peer);
    network.update(&mut app, now);
    assert_no_packet(&server);
    assert_no_packet(&peer);
    send_packet(
        &server,
        "bit-to-byte/1\nhello\nbob\nBob",
        client_address(&network),
    );
    network.update(&mut app, now + DISCOVER_EVERY);
    assert_eq!(read_packet(&server).0, "DISCOVER bob");
    assert!(read_packet(&peer).0.starts_with("FROM "));
}

#[test]
fn verified_peers_stay_online_during_relay_outages_and_goodbye_removes_direct_routes() {
    let (server, mut network) = test_network();
    let peer = UdpSocket::bind("127.0.0.1:0").expect("peer socket");
    let peer_address = peer.local_addr().expect("address");
    let now = Instant::now();
    let mut app = App::new();
    network.handle_packet(
        &mut app,
        &format!("PEER bob {} Bob", peer_address),
        None,
        now,
    );
    let (_, token) = network
        .direct
        .get_mut("bob")
        .expect("known peer")
        .probe(now)
        .expect("due probe");
    send_packet(
        &peer,
        &incoming(&network, "bob", &format!("{} PUNCH_ACK", token)),
        client_address(&network),
    );
    network.receive_packets(&mut app, now);
    network.next_register = now + REGISTER_EVERY;
    network.last_server_reply = Some(now - RELAY_SILENT_AFTER);
    app.people[0].last_seen = now - presence::OFFLINE_AFTER;
    network.update(&mut app, now);
    assert_eq!(
        app.people.len(),
        1,
        "live direct peer survives a silent relay"
    );
    assert_eq!(app.network_status, "Relay unavailable · retrying");
    assert_eq!(
        network.direct_address("bob", Instant::now()),
        Some(peer_address)
    );

    queue_chat(&mut network, &mut app, "bob", "pending message", now);
    assert_eq!(network.pending.len(), 1);
    send_packet(
        &server,
        "bit-to-byte/1\ngoodbye\nbob\nBob",
        client_address(&network),
    );
    network.update(&mut app, now);
    assert!(app.people.is_empty());
    assert!(network.pending.is_empty());
    assert_eq!(network.direct_address("bob", Instant::now()), None);
    assert!(!network.direct.contains_key("bob"));
}

#[test]
fn direct_peers_are_bounded_and_follow_online_presence() {
    let (_server, mut network) = test_network();
    let now = Instant::now();
    let mut app = App::new();
    for number in 0..MAX_PEERS + 1 {
        network.handle_packet(
            &mut app,
            &format!("bit-to-byte/1\nhello\npeer-{}\nMember", number),
            None,
            now,
        );
    }
    network.sync_peers(&app, now);
    assert_eq!(network.direct.len(), MAX_PEERS);
    app.remove_person(0);
    network.sync_peers(&app, now);
    assert_eq!(network.direct.len(), MAX_PEERS);
    assert!(!network.direct.contains_key("peer-0"));
    assert!(network.direct.contains_key(&format!("peer-{}", MAX_PEERS)));
}
