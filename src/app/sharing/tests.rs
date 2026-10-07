use super::*;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[path = "../../../relay/protocol.rs"]
mod relay;

#[tokio::test]
async fn direct_and_relay_tunnels_authenticate_forward_parallel_bytes_and_stop() {
    for blocked in [false, true] {
        timeout(Duration::from_secs(25), async {
            let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let address = socket.local_addr().unwrap();
            let relay_task = tokio::spawn(async move {
                let mut server = relay::Relay::default();
                let mut buffer = [0; 4096];
                loop {
                    let (len, source) = socket.recv_from(&mut buffer).await.unwrap();
                    for (address, mut packet) in
                        server.handle(&buffer[..len], source, Instant::now())
                    {
                        // Unreachable candidates force fallback through the real relay protocol.
                        if blocked && packet.starts_with(b"QUIC PEER ") {
                            let fields: Vec<_> = std::str::from_utf8(&packet)
                                .unwrap()
                                .splitn(5, ' ')
                                .collect();
                            let candidate: SocketAddr = fields[3].parse().unwrap();
                            packet = format!(
                                "QUIC PEER {} 127.0.0.2:{} {}",
                                fields[2],
                                candidate.port(),
                                fields[4]
                            )
                            .into_bytes();
                        }
                        socket.send_to(&packet, address).await.unwrap();
                    }
                }
            });
            let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut host = Sharing {
                relay: Some(address),
                ..Sharing::default()
            };
            host.command = Some(Command::Host {
                person_id: None,
                port: backend.local_addr().unwrap().port(),
            });
            let offer = loop {
                if let Some((_, _, body)) = host.update() {
                    break Offer::parse(&body).unwrap();
                }
                assert!(!host.status.starts_with("share failed:"), "{}", host.status);
                tokio::time::sleep(Duration::from_millis(5)).await;
            };
            let mut wrong_certificate = offer.clone();
            wrong_certificate.certificate =
                rcgen::generate_simple_self_signed(vec!["club-share".into()])
                    .unwrap()
                    .cert
                    .der()
                    .to_vec();
            let mut wrong_token = offer.clone();
            wrong_token.token[0] ^= 1;
            let mut clients = Vec::new();
            for (index, offer) in [wrong_certificate, wrong_token, offer.clone(), offer]
                .into_iter()
                .enumerate()
            {
                let local = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let port = local.local_addr().unwrap().port();
                drop(local);
                let mut client = Sharing {
                    relay: Some(address),
                    ..Sharing::default()
                };
                client.command = Some(Command::Join {
                    person_id: "host".into(),
                    offer,
                    port,
                });
                let status = if index < 2 {
                    "share failed:"
                } else {
                    "http://localhost:"
                };
                loop {
                    client.update();
                    if client.status.starts_with(status) {
                        break;
                    }
                    assert!(
                        index < 2 || !client.status.starts_with("share failed:"),
                        "{}",
                        client.status
                    );
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                if index >= 2 {
                    assert!(client.status.contains(if blocked {
                        "· relay ·"
                    } else {
                        "· direct ·"
                    }));
                    clients.push((client, port));
                }
            }
            let mut streams = JoinSet::new();
            for (_, port) in &clients {
                let mut socket = TcpStream::connect(("127.0.0.1", *port)).await.unwrap();
                socket.write_all(b"request").await.unwrap();
                socket.shutdown().await.unwrap();
                let (mut remote, _) = backend.accept().await.unwrap();
                streams.spawn(async move {
                    let body: Vec<_> = (0..=255).cycle().take(64 * 1024).collect();
                    let mut request = Vec::new();
                    remote.read_to_end(&mut request).await.unwrap();
                    assert_eq!(request, b"request");
                    let (_, received) = tokio::join!(
                        async {
                            remote.write_all(&body).await.unwrap();
                            remote.shutdown().await.unwrap();
                        },
                        async {
                            let mut response = Vec::new();
                            socket.read_to_end(&mut response).await.unwrap();
                            response
                        }
                    );
                    assert_eq!(received, body);
                });
            }
            while let Some(result) = streams.join_next().await {
                result.unwrap();
            }
            for (mut client, port) in clients {
                client.command = Some(Command::Stop);
                client.update();
                while TcpListener::bind(("127.0.0.1", port)).await.is_err() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
            }
            relay_task.abort();
        })
        .await
        .expect("local sharing completed");
    }
}
