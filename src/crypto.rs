use crate::message::{EncryptedPayload, HandshakePayload, Message, MessageStream};

use aes_gcm::aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Key, Nonce}; // Or `Aes128Gcm`
use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures::prelude::*;
use rand::{thread_rng, Rng};
use spake2::{Ed25519Group, Identity, Password, Spake2};

pub async fn handshake(
    stream: &mut MessageStream,
    up: bool,
    password: Bytes,
    id: Bytes,
) -> Result<(&mut MessageStream, Bytes)> {
    let (s1, outbound_msg) =
        Spake2::<Ed25519Group>::start_symmetric(&Password::new(password), &Identity::new(&id));
    println!("client - sending handshake msg");
    let handshake_msg = Message::HandshakeMessage(HandshakePayload {
        up,
        id,
        msg: Bytes::from(outbound_msg),
    });
    println!("client - handshake msg, {:?}", handshake_msg);
    stream.send(handshake_msg).await?;
    let first_message = match stream.next().await {
        Some(Ok(msg)) => match msg {
            Message::HandshakeMessage(response) => response.msg,
            _ => return Err(anyhow!("Expecting handshake message response")),
        },
        _ => {
            return Err(anyhow!("No response to handshake message"));
        }
    };
    println!("client - handshake msg responded to");
    let key = match s1.finish(&first_message[..]) {
        Ok(key_bytes) => key_bytes,
        Err(e) => return Err(anyhow!(e.to_string())),
    };
    println!("Handshake successful. Key is {:?}", key);
    return Ok((stream, Bytes::from(key)));
}

pub fn new_cypher(key: Bytes) -> Aes256Gcm {
    let key = Key::from_slice(&key[..]);
    Aes256Gcm::new(key)
}

const NONCE_SIZE_IN_BYTES: usize = 96 / 8;

pub fn encrypt(cipher: &Aes256Gcm, body: Bytes) -> Result<EncryptedPayload> {
    let mut arr = [0u8; NONCE_SIZE_IN_BYTES];
    thread_rng().try_fill(&mut arr[..])?;
    let nonce = Nonce::from_slice(&arr);
    let plaintext = body.as_ref();
    match cipher.encrypt(nonce, plaintext) {
        Ok(ciphertext) => Ok(EncryptedPayload {
            nonce: Bytes::from(&arr[..]),
            body: Bytes::from(ciphertext.as_ref()),
        }),
        Err(_) => anyhow!("Encryption error"),
    }
}

pub fn decrypt(cipher: &Aes256Gcm, payload: EncryptedPayload) -> Result<Bytes> {
    let nonce = Nonce::from_slice(payload.nonce.as_ref());
    match cipher.decrypt(nonce, payload.body.as_ref()) {
        Ok(_) => Ok(Bytes::from("hello")),
        Err(_) => anyhow!("Decryption error"),
    }
}
