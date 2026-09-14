# Bit to Byte

A small Rust TUI with automatic LAN discovery and online/offline status.

## Run

```bash
cargo run --locked -- --name Alice
```

Use a terminal at least **64 columns by 16 rows** and a current stable Rust
toolchain. `--name` sets your display name; it defaults to `Club member`.
Use `--help` to see the available option.

People are discovered automatically. **Chat messages still stay on this
computer** and are not saved. Announcements and Files are placeholders.

## Try online status on one computer

Leave Alice running, then open a second terminal in the project folder:

```bash
cargo run --locked -- --name Bob
```

Press Tab in both windows to open Messages. Within a few seconds, Alice should
see Bob as `[online]`, and Bob should see Alice as `[online]`. Each app hides
itself from its own list. No IP address or `--connect` option is needed.

Quit Bob with `q` while browsing or Ctrl+C at any time. Alice should now show
Bob as `[offline]`. The conversation and draft remain in Alice's list.
If Bob is killed without a goodbye, Alice marks him offline eight seconds
after his last hello packet. Try a third window to see that people leave
independently.

Each launch has a new ID. Restarting Bob creates a new entry; the old entry
stays offline. Two people can use the same display name without sharing status.
Persistent identities and merging conversations can be added later.

## How discovery works

Every app joins the same IPv4 UDP multicast group, `239.255.42.99:47001`, using
the operating system's default multicast interface. It also sends a copy over
loopback so windows on the same computer can discover each other when Wi-Fi
multicast is filtered. Duplicate hellos refresh the same person. There is no
main machine.

1. Every two seconds, send a `hello` with this app's ID and display name.
2. Receiving a hello adds that person or refreshes their online status.
3. Quitting sends a `goodbye`, which marks that person offline.
4. Eight seconds without a hello also marks that person offline.

These are automatic background packets. Users do not post anything in the
Announcements tab or enter another person's address to be discovered.

UDP packets can be lost; repeated hellos and the timeout handle that.
On two computers, run the same commands on the same local network. The network
and firewall must allow UDP multicast on port 47001. Some school Wi-Fi networks
block multicast or communication between laptops. A successful test on one
computer does not check those network rules. A VPN can also change which
interface the operating system uses.

Discovery sends names without authentication or encryption. An online
label means a recent hello was received, not that TCP chat is connected.
TCP is still demonstrated separately in the examples below.

## If two computers cannot discover each other

A successful two-window test only proves local discovery. The app sends a
separate loopback copy, so that test can pass while a firewall blocks the LAN
packets.

On Ubuntu, check the firewall and recent discovery drops:

```bash
sudo ufw status verbose
sudo journalctl -k --since '5 minutes ago' --grep 'UFW.*DPT=47001'
```

If UFW blocks the packets, allow only the discovery group and UDP port from
your local subnet. For example, this rule is for a `192.168.0.0/24` home network;
replace that subnet if yours is different:

```bash
sudo ufw allow in proto udp from 192.168.0.0/24 to 239.255.42.99 port 47001 comment 'Bit to Byte discovery'
```

The rest of the firewall stays enabled. See the
[UFW rule documentation](https://manpages.ubuntu.com/manpages/noble/man8/ufw.8.html).

WSL 2 normally uses a virtual NAT network. Do not assume an app running inside
WSL is directly on the home LAN. Check the WSL version in Windows PowerShell:

```powershell
wsl --list --verbose
```

On Windows 11 22H2 or newer, WSL's mirrored networking mode supports multicast
and direct LAN access. Windows and Hyper-V firewall rules still apply. Follow
[Microsoft's WSL networking guide](https://learn.microsoft.com/en-us/windows/wsl/networking#mirrored-mode-networking)
when configuring it. A native Windows build is another option for testing
without WSL's virtual network.

## TCP examples for the meeting

There are also two standalone programs that exchange real messages. Start the
receiver in one terminal and the sender in another:

```bash
cargo run --locked --example receiver
cargo run --locked --example sender
```

For another laptop, pass its receiving address to the sender:

```bash
cargo run --locked --example sender -- 192.168.1.24:7000
```

Replace that example IP with the receiving laptop's LAN address. Read the
[TCP meeting guide](examples/tcp/README.md) for setup, a code walkthrough, and
steps toward an online list. These examples are separate from the TUI.

## Iroh connection experiment

For automatic LAN address discovery with relays disabled, start:

```bash
cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-receiver -- --lan
```

Copy its printed sender command to the other computer. No manual IP address
or internet connection is needed after building, but the network must allow
multicast discovery and direct UDP traffic. Wi-Fi client isolation can block
both; manual IP entry does not bypass it.

To test peer-to-peer connections between Ubuntu and WSL using iroh's NAT
traversal and relay support, start the separate receiver:

```bash
cargo run --locked --manifest-path examples/iroh/Cargo.toml --bin iroh-receiver
```

It prints the sender command to run on the other computer. Read the
[iroh experiment guide](examples/iroh/README.md) for the two-computer test and a
relay-only test. This experiment needs Rust 1.91 or newer; the relay-assisted
mode also needs internet access.
Its dependencies and lockfile live in `examples/iroh/`; the TUI still uses its
existing discovery code.

## Controls

| Key | Action |
| --- | --- |
| Tab | Switch between Announcements, Messages, and Files |
| Up / Down | Select a person in Messages |
| Enter | Start typing, or add the typed message locally |
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

The standalone networking examples live together in `examples/tcp/`. The TUI
source remains organized as follows:

```text
src/
  main.rs                Entry point: launch the app
  app/
    mod.rs               Startup, shutdown, and the event loop
    state.rs             Application data, discovered people, and drafts
    presence.rs          Update people and mark missing people offline
    discovery/
      mod.rs             Send and receive automatic discovery packets
      socket.rs          Open and configure the UDP sockets
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
tests stay together in `src/tests/`.

To walk through discovery at a meeting, read these in order:

1. `app/state.rs`: a person has a name, online status, and last-seen time.
2. `app/presence.rs`: a hello marks someone online; a goodbye or timeout marks
   them offline.
3. `app/discovery/mod.rs`: `update` sends a hello when due, receives packets,
   and checks for missing people.
4. `app/discovery/socket.rs`: the operating-system settings that make local
   network discovery and multiple windows work.

For example, Enter produces a Crossterm event. The loop in `src/app/mod.rs`
passes it to `src/tui/input.rs`, which updates the selected person's messages.
The next loop calls `src/tui/render.rs` to draw that change.

The dependencies are **Ratatui** for drawing, **Crossterm** for terminal events,
and **socket2** for configuring the discovery socket before binding it. Sharing
the multicast port lets two windows run on the same computer. See the
[socket2 documentation](https://docs.rs/socket2/0.6.5/socket2/struct.Socket.html)
for these socket options.

The same event loop updates discovery and draws the screen. It waits up to
200 milliseconds for keyboard input between updates. There are no extra app
threads or asynchronous functions.

## Checks

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The code uses named fields, explicit returns, ordinary loops, and match branches.
There are no question-mark error operators, if-let shortcuts, custom macros, or
iterator chains. The small callbacks in drawing and terminal cleanup are required
by the libraries. See CONTRIBUTING.md for the same conventions.
