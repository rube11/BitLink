use super::*;

fn test_peer(now: Instant) -> DirectPeer {
    return DirectPeer {
        address: None,
        next_discovery: now,
        next_probe: now,
        challenge: None,
        verified_at: None,
    };
}

#[test]
fn probes_retry_on_schedule_with_a_fresh_challenge() {
    let now = Instant::now();
    let address = SocketAddr::from(([127, 0, 0, 1], 12001));
    let mut peer = test_peer(now);
    assert!(peer.probe(now).is_none());
    peer.introduce(address, now);
    let (target, first_token) = peer.probe(now).expect("first probe");
    assert_eq!(target, address);
    assert!(peer.probe(now).is_none());
    let (target, next_token) = peer.probe(now + PROBE_EVERY).expect("retry probe");
    assert_eq!(target, address);
    assert_ne!(first_token, next_token);
}

#[test]
fn unchanged_endpoints_preserve_probe_timing_and_changed_endpoints_restart_it() {
    let now = Instant::now();
    let address = SocketAddr::from(([127, 0, 0, 1], 12001));
    let moved = SocketAddr::from(([127, 0, 0, 1], 12002));
    let mut peer = test_peer(now);
    peer.introduce(address, now);
    peer.probe(now).expect("first probe");
    peer.introduce(address, now);
    assert!(peer.probe(now).is_none());
    peer.introduce(moved, now);
    assert_eq!(peer.probe(now).expect("probe new endpoint").0, moved);
}

#[test]
fn invalid_candidates_are_not_probed() {
    let now = Instant::now();
    let mut peer = test_peer(now);
    for invalid in [
        "0.0.0.0:1",
        "127.0.0.1:0",
        "224.0.0.1:1",
        "255.255.255.255:1",
        "[::1]:1",
    ] {
        peer.introduce(invalid.parse().expect("address"), now);
        assert!(peer.probe(now).is_none());
    }
}
