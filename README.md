# Bit to Byte

A Rust terminal messenger using the club's public Lightsail relay.

## Run

On each computer, from this branch:

```bash
cargo run
```

Internet access and a current stable Rust toolchain are required. Use a terminal
at least **64 columns by 16 rows**. Each launch automatically connects to
`32.189.170.75:47002`, registers a fresh ID, and chooses a name such as
`Member a1b2c3`. No server command, IP entry, peer ID, or home-router forwarding
is required. Optionally choose a display name with `cargo run -- --name Alice`.

Press **Tab** to open Messages. Other users running this branch should appear
within a few seconds, including users on different internet connections. Select
a person with Up/Down, press Enter to type, then Enter to send.

The footer shows relay connectivity. Outgoing messages show **sending**,
**delivered** (the receiving app returned a receipt), or **unconfirmed** (no
receipt within ten seconds). An unconfirmed message might still have arrived.
Messages retry during that window and duplicate receipts/messages are handled
in the clients. This is a bounded demo, not guaranteed or ordered delivery.

## Presence and limits

Each client uses one IPv4 UDP socket, refreshing its named registration every
two seconds. The server returns recent named registrations as the People list.
Quit sends a goodbye; crashes or lost connectivity become offline after roughly
14 seconds. If the relay stops responding, the footer says it is unavailable and
the app retries without closing. Existing conversations and drafts remain.

All users of this shared relay can see the names of active app users. This demo
has **no encryption or authenticated identity**. Messages go through the relay. Networks
that block outbound UDP can prevent connection. History is in memory only, IDs
change on restart, and there is no offline delivery. Announcements and Files
remain placeholders.

The server stores only expiring registrations and display names. Message IDs,
receipts, retries, and duplicate tracking live in the clients.
See [deployment details](relay/DEPLOYMENT.md) for server administration.

## Relay server

The relay source, tests, service file, and deployment instructions live in
[relay/](relay/README.md). It handles named registrations, presence, goodbyes,
and message forwarding. To run the server:

```bash
cargo run --locked --bin udp-server
```

Plain `cargo run` still launches the messenger and connects to the public relay.

## Controls

| Key | Action |
| --- | --- |
| Tab | Switch between Announcements, Messages, and Files |
| Up / Down | Select a person in Messages |
| Enter | Start typing, or send the typed message |
| Backspace | Remove the last character |
| Esc | Stop typing and keep the draft |
| q | Quit while browsing |
| Ctrl+C | Quit at any time |

Each person keeps a separate draft. While typing, letters such as q are ordinary
text. Drafts allow up to 500 Unicode character values; pasted newlines become
spaces.

Announcements are read-only sample posts. Files is a placeholder. Messages show
a sender label above each line of text and keep the newest entries visible;
long message text is clipped to the panel width. The composer scrolls horizontally to keep the end of the draft
visible. Full text editing and history scrolling can be added later. Backspace
uses Rust's String::pop, so a combined emoji or accent can take multiple presses.

## Code layout

The messenger lives in `src/`, and the server lives in `relay/`.
`--fizzbuzz` still runs the original exercise.

```text
src/
  main.rs                Entry point: launch the app
  app/
    mod.rs               Startup, shutdown, and the event loop
    state.rs             People, messages, delivery status, and drafts
    presence.rs          Update people and mark missing people offline
    network.rs           One UDP socket, registration, chat, receipts, and retries
    network/tests.rs     Network behavior and real-socket client tests
  tui/
    mod.rs               Declare the terminal modules
    input.rs             Keyboard and paste handling
    render.rs            Ratatui layout and drawing
    theme.rs             Shared colors for the interface
    terminal.rs          Terminal setup and cleanup
  tests/
    mod.rs               Shared test helpers
    keyboard.rs          Input, drafts, and person selection tests
    presence.rs          Hello/goodbye packets, timeouts, and online/offline tests
    rendering.rs         Layout, resizing, and visible-content tests
```

In Rust, `mod.rs` is the entry point for a folder's module. Read `src/main.rs`,
then `src/app/mod.rs` and `src/app/state.rs`. Terminal details live in `src/tui/`;
UI/presence tests live in `src/tests/`. Server tests live in `relay/tests.rs`.

To walk through messaging at a meeting, read these in order:

1. `app/state.rs`: each person has messages and a draft. A message's text, author,
   and delivery status are separate fields.
2. `app/presence.rs`: a hello marks someone online; a goodbye or timeout marks
   them offline.
3. `app/network.rs`: `update` calls a short sequence of named steps to refresh
   registration, read packets, queue messages, retry, and forget old duplicates.
4. `relay/server.rs`: registration-based presence and message routing.

For example, Enter produces a Crossterm event. The loop in `src/app/mod.rs`
passes it to `src/tui/input.rs`, which adds the selected person's message to the
outbox. The next loop sends it through `app/network.rs` and draws its delivery
status through `src/tui/render.rs`.

The dependencies include **Ratatui** for drawing, **Crossterm** for terminal events,
and **getrandom** for fresh client IDs. Networking uses the standard UDP socket.

The same event loop updates the network and draws the screen. It waits up to
200 milliseconds for keyboard input between updates. There are no extra app
threads or asynchronous functions.

## Checks

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --bins
python3 relay/test_relay.py
```

The code uses named fields, explicit returns, ordinary loops, and match branches.
There are no question-mark error operators, if-let shortcuts, custom macros, or
iterator chains. The small callbacks in drawing and terminal cleanup are required
by the libraries.
