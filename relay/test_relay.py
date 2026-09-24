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

            # The server forwards repeats; the receiving app deduplicates them.
            message = "😀" * 500
            for _repeat in range(2):
                send(alice, server_address, f"RELAY bob test-1 CHAT {message}")
                expect(bob, server_address, f"FROM alice test-1 CHAT {message}")
                send(bob, server_address, "RELAY alice test-1 RECEIPT")
                expect(alice, server_address, "FROM bob test-1 RECEIPT")

            send(outsider, server_address, "RELAY bob forged CHAT ignore this")
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
            print("PASS: named presence, Unicode chat, receipts, source checks, goodbye")
    finally:
        server.terminate()
        server.wait(timeout=5)


if __name__ == "__main__":
    main()
