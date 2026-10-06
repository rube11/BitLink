// Both computers read the same random 32-byte key from club.key.
// Only this module knows how to encrypt, decrypt, and encode packets.

use std::io;

use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

pub fn load_key() -> io::Result<[u8; 32]> {
    let bytes = match std::fs::read("club.key") {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(io::Error::new(
                error.kind(),
                "Could not read club.key. Both computers need the same key file; see README.md.",
            ));
        }
    };
    if bytes.len() != 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "club.key must contain exactly 32 random bytes, not a password.",
        ));
    }
    let mut key = [0_u8; 32];
    key.copy_from_slice(&bytes);
    return Ok(key);
}

pub fn encrypt(key: &[u8; 32], sender: &str, recipient: &str, text: &str) -> io::Result<String> {
    // A fixed-size key cannot have an invalid length.
    let cipher = XChaCha20Poly1305::new(key.into());
    // A fresh random nonce lets us safely use the shared key for many messages.
    // It is sent with the ciphertext and does not need to be secret.
    let mut nonce_bytes = [0_u8; 24];
    match getrandom::fill(&mut nonce_bytes) {
        Ok(()) => {}
        Err(_) => return Err(io::Error::other("Could not generate an encryption nonce.")),
    }
    let nonce = XNonce::from(nonce_bytes);
    // Bind the ciphertext to its sender and recipient so the relay cannot
    // change these IDs or turn a message around and reflect it to its sender.
    let context = format!("club-chat/1 {} {}", sender, recipient);
    let payload = Payload {
        msg: text.as_bytes(),
        aad: context.as_bytes(),
    };
    let ciphertext = match cipher.encrypt(&nonce, payload) {
        Ok(ciphertext) => ciphertext,
        Err(_) => return Err(io::Error::other("Could not encrypt the message.")),
    };
    let mut packet = nonce_bytes.to_vec();
    packet.extend_from_slice(&ciphertext);
    // The relay accepts UTF-8 text, so encode the encrypted bytes as Base64.
    return Ok(STANDARD.encode(&packet));
}

pub fn decrypt(key: &[u8; 32], sender: &str, recipient: &str, encoded: &str) -> io::Result<String> {
    let packet = match STANDARD.decode(encoded) {
        Ok(packet) => packet,
        Err(_) => return Err(io::Error::other("Invalid encrypted packet.")),
    };
    // Each packet needs a 24-byte nonce and a 16-byte authentication tag.
    if packet.len() < 40 {
        return Err(io::Error::other("Encrypted packet is too short."));
    }
    // A fixed-size key cannot have an invalid length.
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0_u8; 24];
    nonce_bytes.copy_from_slice(&packet[..24]);
    let nonce = XNonce::from(nonce_bytes);
    let context = format!("club-chat/1 {} {}", sender, recipient);
    let payload = Payload {
        msg: &packet[24..],
        aad: context.as_bytes(),
    };
    let plaintext = match cipher.decrypt(&nonce, payload) {
        Ok(plaintext) => plaintext,
        Err(_) => return Err(io::Error::other("Wrong key or altered packet.")),
    };
    match String::from_utf8(plaintext) {
        Ok(text) => return Ok(text),
        Err(_) => return Err(io::Error::other("Decrypted message is not UTF-8.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_uses_a_fresh_nonce_and_rejects_the_wrong_key_or_identity() {
        let key = [7_u8; 32];
        let first = encrypt(&key, "alice", "bob", "message-1 CHAT hello 界").expect("encrypt");
        let second = encrypt(&key, "alice", "bob", "message-1 CHAT hello 界").expect("encrypt");
        assert_ne!(first, second);
        assert_eq!(
            decrypt(&key, "alice", "bob", &first).expect("decrypt"),
            "message-1 CHAT hello 界"
        );
        assert!(decrypt(&[8_u8; 32], "alice", "bob", &first).is_err());
        assert!(decrypt(&key, "mallory", "bob", &first).is_err());
        assert!(decrypt(&key, "alice", "mallory", &first).is_err());
        assert!(decrypt(&key, "bob", "alice", &first).is_err());
    }

    #[test]
    fn altered_truncated_and_plaintext_packets_are_rejected() {
        let key = [7_u8; 32];
        let encrypted = encrypt(&key, "alice", "bob", "message-1 RECEIPT").expect("encrypt");
        let original = STANDARD.decode(&encrypted).expect("decode");
        for index in [0, 24, original.len() - 1] {
            let mut altered = original.clone();
            altered[index] ^= 1;
            assert!(decrypt(&key, "alice", "bob", &STANDARD.encode(&altered)).is_err());
        }
        assert!(decrypt(&key, "alice", "bob", &STANDARD.encode(&original[..39])).is_err());
        assert!(decrypt(&key, "alice", "bob", "hello").is_err());
        assert!(decrypt(&key, "alice", "bob", "").is_err());
    }
}
