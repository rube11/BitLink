use super::*;
use crate::tests::press;
use crossterm::event::KeyCode;

#[test]
fn global_chat_uses_both_routes_and_waits_for_every_recipient() {
    let (server, mut alice) = test_network();
    let address = server.local_addr().unwrap().to_string();
    let mut bob = Network::connect(Some("Bob Smith".into()), &address, TEST_KEY).unwrap();
    let mut a = App::new();
    let mut b = App::new();
    let now = Instant::now();
    let alice_address =
        SocketAddr::from(([127, 0, 0, 1], alice.socket.local_addr().unwrap().port()));
    let bob_address = SocketAddr::from(([127, 0, 0, 1], bob.socket.local_addr().unwrap().port()));
    alice.handle_packet(
        &mut a,
        &format!("PEER {} {} Bob Smith", bob.id, bob_address),
        None,
        now,
    );
    alice.handle_packet(&mut a, "bit-to-byte/1\nhello\ncarol\nCarol", None, now);
    bob.handle_packet(
        &mut b,
        &format!("PEER {} {} Alice", alice.id, alice_address),
        None,
        now,
    );
    alice.direct.get_mut(&bob.id).unwrap().verified_at = Some(now);
    a.global_draft = "hello everyone".into();
    a.typing = true;
    press(&mut a, KeyCode::Enter);
    alice.queue_outgoing_messages(&mut a, now);
    alice.resend_pending_messages(&mut a, now);
    assert_eq!(a.global_messages.len(), 1);
    assert_eq!(a.global_messages[0].route, Some("D/R"));
    let (forward, _) = read_packet(&server);
    let encrypted = forward.strip_prefix("RELAY carol ENC1 ").unwrap();
    assert_eq!(
        crypto::decrypt(&TEST_KEY, &alice.id, "carol", encrypted).unwrap(),
        format!("{} GLOBAL hello everyone", alice.pending[1].token)
    );
    let duplicate = format!("FROM {} {}", alice.id, alice.pending[0].encrypted_payload);
    bob.receive_packets(&mut b, now);
    bob.handle_packet(&mut b, &duplicate, None, now);
    assert_eq!(b.global_messages.len(), 1);
    assert_eq!(b.global_messages[0].author.as_deref(), Some("Alice"));
    assert!(b.people[0].messages.is_empty());
    for _ in 0..2 {
        let (receipt, _) = read_packet(&server);
        let receipt = receipt
            .strip_prefix(&format!("RELAY {} ", alice.id))
            .unwrap();
        alice.handle_packet(&mut a, &format!("FROM {} {}", bob.id, receipt), None, now);
    }
    assert_eq!(a.global_messages[0].pending_receipts, 1);
    assert_eq!(a.global_messages[0].delivery, Some(Delivery::Sending));
    alice.resend_pending_messages(&mut a, now + RESEND_EVERY);
    assert_eq!(read_packet(&server).0, forward);
    let receipt = incoming(
        &alice,
        "carol",
        &format!("{} RECEIPT", alice.pending[0].token),
    );
    alice.handle_packet(&mut a, &receipt, None, now);
    assert_eq!(a.global_messages[0].delivery, Some(Delivery::Delivered));
    assert!(alice.pending.is_empty());
}

#[test]
fn global_departures_and_timeouts_never_report_complete_delivery() {
    for queued in [true, false] {
        let (_server, mut network) = test_network();
        let mut app = App::new();
        let now = Instant::now();
        for name in ["bob", "carol"] {
            network.handle_packet(
                &mut app,
                &format!("bit-to-byte/1\nhello\n{name}\n{name}"),
                None,
                now,
            );
        }
        app.global_draft = "hello".into();
        app.typing = true;
        press(&mut app, KeyCode::Enter);
        if !queued {
            network.queue_outgoing_messages(&mut app, now);
        }
        network.handle_packet(&mut app, "bit-to-byte/1\ngoodbye\nbob\nbob", None, now);
        assert!(app.typing);
        network.queue_outgoing_messages(&mut app, now);
        let receipt = incoming(
            &network,
            "carol",
            &format!("{} RECEIPT", network.pending[0].token),
        );
        network.handle_packet(&mut app, &receipt, None, now);
        assert_eq!(app.global_messages[0].pending_receipts, 0);
        assert_eq!(app.global_messages[0].delivery, Some(Delivery::Unconfirmed));
        app.global_draft = "another".into();
        press(&mut app, KeyCode::Enter);
        network.queue_outgoing_messages(&mut app, now);
        network.resend_pending_messages(&mut app, now + GIVE_UP_AFTER);
        assert_eq!(app.global_messages[1].delivery, Some(Delivery::Unconfirmed));
        network.handle_packet(&mut app, "bit-to-byte/1\ngoodbye\ncarol\ncarol", None, now);
        app.global_draft = "keep draft".into();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.global_draft, "keep draft");
        assert!(network.pending.is_empty());
    }
}
