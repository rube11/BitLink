"""Check the built relay with two local UDP clients. No internet access needed."""

import socket
import subprocess
import time
from pathlib import Path


def client():
    connection = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    connection.bind(("127.0.0.1", 0))
    connection.settimeout(1)
    return connection


def send(connection, server, text):
    connection.sendto(text.encode("utf-8"), server)


def expect(connection, server, text):
    packet, source = connection.recvfrom(4096)
    assert source == server, source
    assert packet.decode("utf-8") == text, packet


def main():
    root = Path(__file__).resolve().parents[1]
    binary = root / "target/debug/udp-server"
    with client() as port_finder:
        server_address = port_finder.getsockname()

    server = subprocess.Popen(
        [str(binary), f"127.0.0.1:{server_address[1]}"],
        stdout=subprocess.DEVNULL,
    )
    try:
        with client() as alice, client() as bob, client() as outsider:
            alice_address = "%s:%s" % alice.getsockname()
            bob_address = "%s:%s" % bob.getsockname()

            # Retry registration while the server process starts.
            deadline = time.monotonic() + 5
            while True:
                send(alice, server_address, "REGISTER alice Alice")
                try:
                    expect(alice, server_address, f"REGISTERED {alice_address}")
                    break
                except socket.timeout:
                    assert server.poll() is None, "Relay exited during startup"
                    assert time.monotonic() < deadline, "Relay did not start"

            send(bob, server_address, "REGISTER bob Bob Smith")
            expect(bob, server_address, f"REGISTERED {bob_address}")
            expect(bob, server_address, "bit-to-byte/1\nhello\nalice\nAlice")
            send(alice, server_address, "REGISTER alice Alice")
            expect(alice, server_address, f"REGISTERED {alice_address}")
            expect(alice, server_address, "bit-to-byte/1\nhello\nbob\nBob Smith")

            send(alice, server_address, "DISCOVER bob")
            expect(alice, server_address, f"PEER bob {bob_address} Bob Smith")
            expect(bob, server_address, f"PEER alice {alice_address} Alice")

            # The server forwards repeats; the receiving app deduplicates them.
            message = "😀" * 500
            for _repeat in range(2):
                send(alice, server_address, f"RELAY bob test-1 CHAT {message}")
                expect(bob, server_address, f"FROM alice test-1 CHAT {message}")
                send(bob, server_address, "RELAY alice test-1 RECEIPT")
                expect(alice, server_address, "FROM bob test-1 RECEIPT")

            send(outsider, server_address, "RELAY bob forged CHAT ignore this")
            send(outsider, server_address, "DISCOVER bob")
            bob.settimeout(0.2)
            try:
                bob.recvfrom(4096)
                raise AssertionError("Relay accepted an unregistered sender")
            except socket.timeout:
                pass

            send(bob, server_address, "GOODBYE bob")
            expect(alice, server_address, "bit-to-byte/1\ngoodbye\nbob\nBob Smith")
            send(alice, server_address, "RELAY bob test-2 CHAT after goodbye")
            expect(alice, server_address, "ERROR peer-unavailable")
            with client() as host, client() as guest:
                send(host, server_address, "QUIC REGISTER host Sharing")
                expect(host, server_address, f"QUIC REGISTERED {'%s:%s' % host.getsockname()}")
                send(guest, server_address, "QUIC REGISTER guest Sharing")
                expect(guest, server_address, f"QUIC REGISTERED {'%s:%s' % guest.getsockname()}")
                expect(guest, server_address, "QUIC bit-to-byte/1\nhello\nhost\nSharing")
                send(guest, server_address, "QUIC DISCOVER host")
                expect(guest, server_address, f"QUIC PEER host {'%s:%s' % host.getsockname()} Sharing")
                expect(host, server_address, f"QUIC PEER guest {'%s:%s' % guest.getsockname()} Sharing")
                payload = bytes(range(256)) * 4
                host.sendto(b"QUIC DATA guest " + payload, server_address)
                assert guest.recvfrom(4096) == (b"QUIC FROM host " + payload, server_address)
                outsider.sendto(b"QUIC DATA guest " + payload, server_address)
                guest.settimeout(0.2)
                try:
                    guest.recvfrom(4096)
                    raise AssertionError("Relay accepted unregistered QUIC traffic")
                except socket.timeout:
                    pass
                # Sharing registrations must not appear in the chat people list.
                send(alice, server_address, "REGISTER alice Alice")
                expect(alice, server_address, f"REGISTERED {alice_address}")
                alice.settimeout(0.2)
                try:
                    alice.recvfrom(4096)
                    raise AssertionError("Sharing endpoints appeared as chat members")
                except socket.timeout:
                    pass
            print("PASS: chat, discovery, receipts, source checks, goodbye, isolated binary QUIC relay")
    finally:
        server.terminate()
        server.wait(timeout=5)


if __name__ == "__main__":
    main()
