# First iroh connection

Two small programs to check whether Ubuntu and WSL can exchange a message
without manual IP addresses, port forwarding, or a separate VPN installation.
This is an experiment, separate from the TUI and the TCP examples.

## Try LAN discovery without a relay

Both computers need **Rust 1.91 or newer** and the same local network with
peer traffic allowed. Internet access is needed to download dependencies for
the first build, but not to run this mode afterward.

Start the receiver from the project folder:

```bash
cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-receiver -- --lan
```

Copy its printed sender command to the other computer. It has this shape:

```bash
cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-sender -- --lan RECEIVER_ID "Hello over the LAN!"
```

The receiver advertises under the mDNS service `bit-to-byte-v1`. The sender
uses mDNS to find the selected receiver's IP addresses, then opens an encrypted
QUIC connection directly. You copy an ID to choose the recipient; there is no
IP address or relay URL to enter. Both endpoints disable relays and public
address lookup services, and there is no relay fallback. A new receiver launch
creates a new ID, so copy a fresh command after restarting it.

Success means the receiver prints your message and the sender prints
`Delivered: the receiver confirmed your message.` Both should report
`Current connection path: direct.` Repeat with another message, then swap
receiver and sender roles. Two terminals on one computer also work when local
multicast is allowed, but do not validate communication between laptops.

This adds automatic address lookup to the separate example, not a people list
or TUI integration. Discovery advertises public connection information on the
LAN; an ID is not a password. The receiver accepts any reachable peer.

### LAN restrictions and troubleshooting

- Keep both computers on the same multicast-capable network. mDNS does not
  normally cross routed subnets. A VPN may select a different interface.
- Host firewalls must allow mDNS (UDP 5353) and the endpoint's UDP traffic.
  The LAN mode uses dynamically assigned QUIC ports and changes no firewall rules.
- WSL 2's default NAT can prevent LAN discovery. Use a native Windows build
  or supported WSL mirrored networking, with Windows and Hyper-V firewall
  permissions as described in the [project networking guide](../../README.md#if-two-computers-cannot-discover-each-other).
- A missing/stale ID or blocked multicast causes discovery to fail. Connection
  and receipt waiting is bounded by 30 seconds; discovery can fail sooner.
  A failed receipt leaves delivery unconfirmed; check the receiver before retrying.
- If only multicast is blocked, the existing explicit-address mode can work:
  start `iroh-receiver --direct LOCAL_IP:47002` and copy its printed command.
  This still requires direct UDP access. `--lan` cannot be combined with
  `--direct`, `--relay-only`, or the receiver's `--allow-from` helper.
- Wi-Fi client isolation can block discovery **and** direct connections.
  Neither mDNS nor manual IP entry bypasses this. Use an authorized network
  that permits peer traffic, such as a private hotspot with isolation disabled,
  or ask the network administrator to permit it.

mDNS discovers local addresses; it does not provide Internet NAT traversal.
This mode does not promise connections across CGNAT, isolated school Wi-Fi,
or networks that block UDP. See the
[mDNS lookup documentation](https://docs.rs/iroh-mdns-address-lookup/0.5.0/iroh_mdns_address_lookup/)
for the library used here.

## Try the existing relay-assisted mode

Both computers need this version of the project, internet access, and **Rust
1.91 or newer**. Check with `rustc --version`. The first build downloads and
compiles iroh and Tokio, so it takes longer than the existing TCP examples.

From the project folder, start the receiver on Ubuntu:

```bash
cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-receiver
```

Wait for `Receiver ready`. It prints a complete sender command containing its
current receiver ID and relay URL. Copy that command into your WSL terminal,
also in the project folder. Keep the receiver running while the sender builds.

The command has this shape; use the actual values printed by your receiver:

```bash
cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-sender -- RECEIVER_ID RELAY_URL "Hello from WSL!"
```

Success looks like this:

- Receiver: `Received: Hello from WSL!`
- Sender: `Delivered: the receiver confirmed your message.`

Run the sender again with a different quoted message. The receiver handles one
sender at a time and stays open between messages. Press Ctrl+C to stop it.
Then swap roles: run the receiver in WSL and the sender on Ubuntu.

You can use two terminals on one computer for the same procedure. That checks
the programs, but only the two-computer test checks your Ubuntu/WSL connection.

## Check the relay path

Both programs print the selected connection path at the time of the message:

- **direct**: iroh has established a direct connection between the computers.
- **relay**: encrypted traffic currently passes through an iroh relay.

This is a snapshot. A short exchange can finish through a relay before iroh
has time to establish a direct path. Seeing `relay` does not prove a direct
connection is impossible.

To deliberately test relaying, add `--relay-only` immediately after the sender
command's `--`:

```bash
cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-sender -- --relay-only RECEIVER_ID RELAY_URL "Testing the relay"
```

This disables direct network paths for that sender only. A successful exchange
then confirms that the public relay can carry a message and its receipt, even
when both programs are on one computer. No firewall settings are changed.

## What happens

1. Each program opens an iroh endpoint and connects to a public relay.
2. The sender uses the receiver's ID and relay URL to contact that endpoint.
3. Iroh establishes an encrypted QUIC connection and attempts a direct path.
   Traffic can continue through a relay if a direct path is unavailable.
4. The sender opens a bidirectional stream, writes one message, and calls
   `finish` to mark the end of the message.
5. The receiver reads the complete message, prints it, and sends `received`
   back over the same stream.
6. The sender checks that receipt before reporting delivery and disconnecting.

An ID identifies a particular iroh endpoint; it is not a username or IP address.
This experiment makes a new identity each time it starts. Copy a fresh command
after restarting the receiver. The ID and relay URL are connection information,
not a password or a private club invitation. Anyone who has them can try sending
a message while this receiver is running.

## If it fails

- **No `Receiver ready`:** the endpoint could not reach a public relay. Check
  internet access and whether the network permits the relay connection. The
  startup wait times out after about 30 seconds.
- **No receipt:** check that the receiver is still open and that its ID and
  relay URL match the command. The connection/message exchange has a separate
  30-second limit. An unconfirmed message might still have reached the receiver;
  inspect its terminal before retrying.
- **Invalid arguments:** copy the whole printed command and keep the message
  in quotes. Messages must contain 1–4096 UTF-8 bytes.

Public relays are shared, rate-limited, and have no uptime guarantee. The
relay-assisted mode intentionally waits for a relay and requires internet access, even
for a same-computer test. It does not promise access through every school
firewall. See [iroh's relay hosting information](https://www.iroh.computer/services/hosting).

## Read the code

| File | Responsibility |
| --- | --- |
| `src/receiver/main.rs` | Print connection details, accept a sender, read a message, send a receipt |
| `src/sender/main.rs` | Read arguments, connect, send a message, check the receipt |
| `src/network.rs` | Shared protocol values, endpoint setup, time limits, and path display |
| `Cargo.toml` / `Cargo.lock` | Dependencies for this experiment only |

Start with `send_message` in `src/sender/main.rs`, then `receive_message` in `src/receiver/main.rs`.
The surrounding `run` functions handle setup, arguments, and timeouts.

Tokio runs the asynchronous networking work. `.await` pauses the current
function while network work completes. `match` handles each result explicitly;
there are no question-mark error operators or custom macros.

The receiver limits message size and escapes terminal control characters before
printing. It processes connections sequentially, with a timeout for a stalled
sender. This is not a full chat system: there is no online list, saved identity,
group membership, history, automatic reconnection, file transfer, or TUI
integration yet. The separate sender and receiver roles help us test; the
eventual app can both accept and initiate connections on one endpoint.

See [iroh's documentation](https://docs.iroh.computer/what-is-iroh) for the
networking library underneath these programs.

## Checks

Run from the project folder:

```bash
cargo fmt --manifest-path examples/iroh/Cargo.toml --check
cargo clippy --locked --manifest-path examples/iroh/Cargo.toml --all-targets -- -D warnings
cargo build --locked --manifest-path examples/iroh/Cargo.toml
```

The project's root Cargo commands do not build or check this separate package.
Use the two-terminal LAN procedure above to check actual message delivery,
then repeat between your two computers. Test the relay-assisted modes
separately if needed; their success does not validate relay-free LAN access.
