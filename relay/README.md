# Club UDP relay

This is the server used by the terminal messenger. Users only need `cargo run`;
the default public endpoint is built into the app. See [DEPLOYMENT.md](DEPLOYMENT.md)
for the deployed instance and administration commands.

## Run a server

```bash
cargo run --locked --bin udp-server -- 0.0.0.0:47002
```

The server uses the first argument as its bind address. If omitted, it listens on
`0.0.0.0:47002`. Running it locally does not change the app's default public endpoint.

To build a replacement for the existing Linux service, on a matching Linux system:

```bash
cargo build --locked --release --bin udp-server
```

Install `target/release/udp-server` at `/opt/club-relay/udp-server` on the relay instance and restart
`club-udp`. The supplied [service file](club-udp.service) runs as the existing
unprivileged `club-relay` user and restarts on failure. Deployment details are
recorded in [DEPLOYMENT.md](DEPLOYMENT.md).

## How it works

The server keeps one small list: each member's ID, name, observed UDP address,
and last registration time. It has three commands:

| Packet from a client | Server action |
| --- | --- |
| `REGISTER id display-name` | Save or refresh that registration, return `REGISTERED observed-address`, and list recent peers. |
| `RELAY recipient-id payload` | Forward `FROM sender-id payload` to the recipient. The sender ID comes from the registered source address. |
| `GOODBYE id` | Remove that source's registration and notify the remaining members. |

Presence packets have four lines: `bit-to-byte/1`, `hello` or `goodbye`,
the person's ID, and their name. A registration less than six seconds old appears
in peer lists; registrations are removed after thirty seconds. Clients refresh
every two seconds and mark missing people offline after eight seconds.

The client sends chat payloads as `message-id CHAT text` and replies with
`message-id RECEIPT`. It owns all message IDs, receipts, retries, duplicate
tracking, and conversations. The server forwards payloads without interpreting
or storing them. No connection setup or per-conversation routing table is needed.

IDs contain 1–40 ASCII letters, digits, underscores, or hyphens. Display names
contain 1–40 characters without control characters. A live ID and source address
cannot be claimed by another registration. The server caps registrations at 128
and relay payloads at 2100 UTF-8 bytes, enough for the app's 500-character messages.

This is an unencrypted, unauthenticated demo. There is no offline queue, durable
history, or direct hole punching. Restarting the server clears registrations;
running apps register again automatically.

## Checks

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --bins
python3 relay/test_relay.py
```

Rust tests cover registration freshness, expiry, address conflicts, invalid
packets, limits, and routing. The Python check starts a local server and uses
real UDP sockets to verify presence, chat, receipts, and goodbye. Client tests
in `src/app/network/tests.rs` cover retries, receipts, duplicate suppression,
timeouts, and packets from unexpected addresses.
