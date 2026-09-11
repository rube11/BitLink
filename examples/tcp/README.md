# First TCP connection

Two small programs, using only Rust's standard library. Keep these separate from
the TUI while learning how a connection works.

## Try it on one computer

Open two terminals in the project folder. Start the receiver first:

```bash
cargo run --locked --example receiver
```

In the second terminal, start the sender:

```bash
cargo run --locked --example sender
```

Type a message in the sender and press Enter. It should appear in the receiver
alongside the sender's address. Send a few more messages, then type `/quit`.
The receiver prints a disconnection notice and waits for another sender.
Press Ctrl+C to stop the receiver.

## Try it on two computers

Run the receiver on one computer with the same command. On the other computer,
use the receiver's actual LAN IPv4 address and port. For example:

```bash
cargo run --locked --example sender -- 192.168.1.24:7000
```

Replace `192.168.1.24` with the receiving computer's address. Find it in the
network settings for that computer's active Wi-Fi or Ethernet connection.

- `127.0.0.1` means this computer, so it is for the two-terminal test.
- `0.0.0.0` tells the receiver to listen on all IPv4 interfaces. It is not the
  address another person should enter as the destination.
- `7000` is the port. Both programs need to use the same port.

You can choose another listening address and port when needed:

```bash
cargo run --locked --example receiver -- 127.0.0.1:8000
cargo run --locked --example sender -- 127.0.0.1:8000
```

That example accepts connections from this computer only. For a LAN test on a
different port, use `0.0.0.0:8000` on the receiver and its actual LAN address on
the sender.

If the local test works but the two-computer test does not, check the address,
port, receiver firewall, and school network rules. SSH access to one machine
does not guarantee that this port or connections between student laptops are
allowed. The TUI's automatic UDP discovery is separate from these examples.

## Read the code

Start with `receiver.rs`:

1. `TcpListener::bind` opens the listening address.
2. `accept` waits for a connection and returns its stream and peer address.
3. `BufReader::read_line` waits for a complete line.
4. Reading zero bytes means the connection was closed.

Then read `sender.rs`:

1. `TcpStream::connect` connects to the receiver.
2. Standard input reads a line from the keyboard.
3. `write_all` writes the entire line to the connection or returns an error.

One newline ends one UTF-8 text message. A TCP read is not necessarily one
message: bytes may arrive in pieces or several lines may arrive together.
`BufReader` handles that buffering. The receiver rejects an unfinished final
line if the sender disconnects without a newline.

## Build on it during the meeting

The receiver currently handles **one sender at a time**. Have one person
disconnect before testing the next person's sender. Everything is plain text;
there is no encryption, saved history, automatic discovery, or TUI integration.

Suggested progression:

1. Send a name as the first line and display it with later messages.
2. Move handling one connection into a clearly named function.
3. Handle multiple connections, then maintain a list of connected people.
4. Show that list in the TUI.

The connection notices are the starting point for an online list. A list of
connected people is different from discovering everyone on the network, and
an abruptly disconnected laptop may take time to be detected. Heartbeats and
discovery are implemented separately in the TUI's `app/discovery/` and
`app/presence.rs` modules.

These receiver and sender roles are for learning. The eventual peer-to-peer app
will listen for connections and initiate connections in the same program.
