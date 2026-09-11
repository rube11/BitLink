use std::time::{Duration, Instant};

use crate::app::presence::{
    OFFLINE_AFTER, is_valid_name, mark_missing_people_offline, receive_packet,
};
use crate::app::state::{App, View};

use super::draw_screen;

#[test]
fn hellos_update_one_person_and_goodbye_preserves_their_conversation() {
    let mut app = App::new();
    let now = Instant::now();
    let hello = b"bit-to-byte/1\nhello\nalice-1\nAlice";
    receive_packet(&mut app, hello, "self", now);
    assert_eq!(app.people.len(), 1);
    assert!(app.people[0].online);
    app.people[0].messages.push(String::from("You: Hello"));
    app.people[0].draft = String::from("Keep this draft");

    receive_packet(&mut app, hello, "self", now + Duration::from_secs(2));
    assert_eq!(app.people.len(), 1);
    assert_eq!(app.people[0].last_seen, now + Duration::from_secs(2));
    receive_packet(&mut app, b"bit-to-byte/1\nhello\nbob-1\nBob", "self", now);
    app.view = View::Messages;
    assert!(draw_screen(&app, 80, 24).contains("[online]"));

    receive_packet(
        &mut app,
        b"bit-to-byte/1\ngoodbye\nalice-1\nAlice",
        "self",
        now + Duration::from_secs(3),
    );
    assert_eq!(app.people.len(), 2);
    assert!(!app.people[0].online);
    assert!(app.people[1].online);
    assert_eq!(app.people[0].messages, vec!["You: Hello"]);
    assert_eq!(app.people[0].draft, "Keep this draft");
    let screen = draw_screen(&app, 80, 24);
    assert!(screen.contains("[offline]"));
    assert!(screen.contains("[online]"));
}

#[test]
fn missed_hellos_expire_and_a_later_hello_restores_the_person() {
    let mut app = App::new();
    let now = Instant::now();
    let hello = b"bit-to-byte/1\nhello\nalice-1\nAlice";
    receive_packet(&mut app, hello, "self", now);
    mark_missing_people_offline(&mut app, now + OFFLINE_AFTER - Duration::from_millis(1));
    assert!(app.people[0].online);
    mark_missing_people_offline(&mut app, now + OFFLINE_AFTER);
    assert!(!app.people[0].online);
    receive_packet(
        &mut app,
        hello,
        "self",
        now + OFFLINE_AFTER + Duration::from_secs(1),
    );
    assert_eq!(app.people.len(), 1);
    assert!(app.people[0].online);
}

#[test]
fn duplicate_names_are_separate_people_and_self_packets_are_ignored() {
    let mut app = App::new();
    let now = Instant::now();
    receive_packet(&mut app, b"bit-to-byte/1\nhello\nself\nAlex", "self", now);
    assert!(app.people.is_empty());
    receive_packet(&mut app, b"bit-to-byte/1\nhello\nalex-1\nAlex", "self", now);
    receive_packet(&mut app, b"bit-to-byte/1\nhello\nalex-2\nAlex", "self", now);
    receive_packet(
        &mut app,
        b"bit-to-byte/1\ngoodbye\nalex-1\nAlex",
        "self",
        now,
    );
    assert_eq!(app.people.len(), 2);
    assert!(!app.people[0].online);
    assert!(app.people[1].online);
}

#[test]
fn unrelated_and_malformed_packets_do_not_add_people() {
    let mut app = App::new();
    let now = Instant::now();
    for text in [
        "",
        "another-app/1\nhello\npeer\nAlice",
        "bit-to-byte/1\nhello",
        "bit-to-byte/1\nhello\npeer\nAlice\nextra",
        "bit-to-byte/1\nunknown\npeer\nAlice",
        "bit-to-byte/1\nhello\n\nAlice",
        "bit-to-byte/1\nhello\npeer\n   ",
        "bit-to-byte/1\nhello\npeer\nAlice\u{1b}",
        "bit-to-byte/1\ngoodbye\nunknown\nAlice",
    ] {
        receive_packet(&mut app, text.as_bytes(), "self", now);
    }
    receive_packet(&mut app, &[255], "self", now);
    assert!(app.people.is_empty());
    assert!(!is_valid_name(&"a".repeat(41)));
    assert!(is_valid_name("Alice 界"));
}
