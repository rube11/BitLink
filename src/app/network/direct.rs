// Only an encrypted reply to our current probe confirms a direct route.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

pub(super) const DISCOVER_EVERY: Duration = Duration::from_secs(10);
const PROBE_EVERY: Duration = Duration::from_secs(2);
pub(super) const DIRECT_SILENT_AFTER: Duration = Duration::from_secs(6);
pub(super) const MAX_PEERS: usize = 128;

pub(super) struct DirectPeer {
    pub address: Option<SocketAddr>,
    pub(super) next_discovery: Instant,
    pub(super) next_probe: Instant,
    pub(super) challenge: Option<String>,
    pub(super) verified_at: Option<Instant>,
}

impl DirectPeer {
    pub fn introduce(&mut self, address: SocketAddr, now: Instant) {
        if !address.is_ipv4()
            || address.port() == 0
            || address.ip().is_unspecified()
            || address.ip().is_multicast()
            || address.ip() == std::net::Ipv4Addr::BROADCAST
        {
            return;
        }
        if self.address != Some(address) {
            self.address = Some(address);
            self.challenge = None;
            self.verified_at = None;
            self.next_probe = now;
        }
    }

    pub fn probe(&mut self, now: Instant) -> Option<(SocketAddr, String)> {
        let address = match self.address {
            Some(address) => address,
            None => return None,
        };
        if now < self.next_probe {
            return None;
        }
        self.next_probe = now + PROBE_EVERY;
        let token = match super::random_id() {
            Ok(token) => token,
            Err(_) => return None,
        };
        self.challenge = Some(token.clone());
        return Some((address, token));
    }
}

#[cfg(test)]
mod tests;
