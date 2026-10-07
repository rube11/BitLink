# Demo deployment

Deployed and verified on 2026-09-24.

- Instance: `club-relay`, Oregon (`us-west-2`), Ubuntu 24.04.
- Plan: $5/month, dual-stack, 512 MB RAM, 2 vCPUs, 20 GB disk.
- Static IP resource: `club-relay-ip`.
- Client endpoint: `32.189.170.75:47002` (IPv4 UDP).
- Service: `club-udp`, enabled at boot, running as `club-relay`.
- Binary: `/opt/club-relay/udp-server`.
- Unit: `/etc/systemd/system/club-udp.service`.
- Lightsail firewall allows UDP 47002 from any IPv4 address. Default HTTP/SSH
  rules remain. Host UFW was inactive; it was not changed.

Run the app on each computer:

```bash
cargo run
```

Names and IDs are generated automatically. Optionally use `cargo run -- --name Alice`.
Press Tab for Messages, select a person, and press Enter to compose and send.
See [README.md](README.md) for limits and protocol.

Deployment verification: two local clients registered through the public endpoint. The
server forwarded a test message and its receipt in opposite directions. Receiver
and sender both reported relay delivery; server logs confirmed both forwards.
This established public relay reachability.

Administration (using the instance's configured Lightsail SSH key):

```bash
ssh -i /path/to/key.pem ubuntu@32.189.170.75
sudo systemctl status club-udp
sudo journalctl -u club-udp -f
```

This is the unencrypted demo described in README.md, not a production messaging
service. No changes were made to the account's other project instance.

## Default app integration

The deployed server also supports named registrations and online presence for
the TUI. Users now run only `cargo run`; no manual server address or peer ID is
needed. The source cleanup retains the TUI wire protocol and removes the unused
standalone UDP client and introduction commands. It does not require changing the
deployed server; no new deployment was made as part of this refactor.
