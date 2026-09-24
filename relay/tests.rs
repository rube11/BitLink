use super::*;

fn address(port: u16) -> SocketAddr {
    return SocketAddr::from(([127, 0, 0, 1], port));
}

fn reply(address: SocketAddr, text: &str) -> Reply {
    return Reply {
        address: address,
        text: String::from(text),
    };
}

#[test]
fn registration_lists_recent_people_and_goodbye_removes_them() {
    let alice = address(11001);
    let bob = address(11002);
    let now = Instant::now();
    let mut server = Server::new();

    server.handle("REGISTER alice Alice", alice, now);
    let replies = server.handle("REGISTER bob Bob Smith", bob, now);
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0], reply(bob, &format!("REGISTERED {}", bob)));
    assert_eq!(replies[1], reply(bob, "bit-to-byte/1\nhello\nalice\nAlice"));

    let later = now + PRESENCE_LIFETIME;
    let replies = server.handle("REGISTER bob Bob Smith", bob, later);
    assert_eq!(replies.len(), 1);
    assert_eq!(server.members.len(), 2);
    assert!(server.handle("GOODBYE bob", alice, later).is_empty());
    assert_eq!(
        server.handle("GOODBYE bob", bob, later),
        vec![reply(alice, "bit-to-byte/1\ngoodbye\nbob\nBob Smith")]
    );
    assert_eq!(server.member_index("bob"), None);
}

#[test]
fn routing_uses_only_live_registrations_and_the_observed_sender() {
    let alice = address(10001);
    let bob = address(10002);
    let outsider = address(10003);
    let now = Instant::now();
    let mut server = Server::new();
    server.handle("REGISTER alice Alice", alice, now);
    server.handle("REGISTER bob Bob", bob, now);

    assert_eq!(
        server.handle("RELAY bob token CHAT hello", alice, now),
        vec![reply(bob, "FROM alice token CHAT hello")]
    );
    assert_eq!(
        server.handle("RELAY alice token RECEIPT", bob, now),
        vec![reply(alice, "FROM bob token RECEIPT")]
    );
    assert!(
        server
            .handle("RELAY bob anything", outsider, now)
            .is_empty()
    );
    assert_eq!(
        server.handle("REGISTER alice Alice", outsider, now),
        vec![reply(outsider, "ERROR id-in-use")]
    );
    assert_eq!(
        server.handle("REGISTER alias Alias", alice, now),
        vec![reply(alice, "ERROR address-in-use")]
    );

    let later = now + REGISTRATION_LIFETIME;
    server.handle("REGISTER alice Alice", alice, later);
    assert_eq!(
        server.handle("RELAY bob anything", alice, later),
        vec![reply(alice, "ERROR peer-unavailable")]
    );
    // After expiry, the same ID can register from a new address.
    server.handle("REGISTER bob Bob", outsider, later);
    assert_eq!(
        server.handle("RELAY bob opaque payload", alice, later),
        vec![reply(outsider, "FROM alice opaque payload")]
    );
}

#[test]
fn malformed_packets_do_not_change_members_or_forward_messages() {
    let alice = address(12001);
    let bob = address(12002);
    let now = Instant::now();
    let mut server = Server::new();

    for packet in [
        "",
        "REGISTER",
        "REGISTER  Alice",
        "REGISTER bad! Alice",
        "REGISTER alice",
        "REGISTER alice   ",
        "REGISTER alice Alice\nBob",
    ] {
        assert!(server.handle(packet, alice, now).is_empty());
        assert!(server.members.is_empty());
    }
    let long_name = format!("REGISTER alice {}", "a".repeat(41));
    assert!(server.handle(&long_name, alice, now).is_empty());

    server.handle("REGISTER alice Alice", alice, now);
    server.handle("REGISTER bob Bob", bob, now);
    for packet in [
        "RELAY bob",
        "RELAY bob ",
        "GOODBYE alice extra",
        "UNKNOWN bob",
    ] {
        assert!(server.handle(packet, alice, now).is_empty());
        assert_eq!(server.members.len(), 2);
    }
    let oversized = format!("RELAY bob {}", "x".repeat(MAX_PAYLOAD_BYTES + 1));
    assert!(server.handle(&oversized, alice, now).is_empty());
}

#[test]
fn a_full_server_still_refreshes_existing_members() {
    let now = Instant::now();
    let mut server = Server::new();
    for number in 0..MAX_MEMBERS {
        let packet = format!("REGISTER person-{} Person {}", number, number);
        server.handle(&packet, address(20000 + number as u16), now);
    }
    assert_eq!(server.members.len(), MAX_MEMBERS);
    assert!(
        server
            .handle("REGISTER extra Extra", address(21000), now)
            .is_empty()
    );
    assert!(
        !server
            .handle("REGISTER person-0 Person 0", address(20000), now)
            .is_empty()
    );
    assert_eq!(server.members.len(), MAX_MEMBERS);
}
