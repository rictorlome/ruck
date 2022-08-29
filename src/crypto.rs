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
    password: Bytes,
    id: Bytes,
) -> Result<(&mut MessageStream, Aes256Gcm)> {
    let (s1, outbound_msg) =
        Spake2::<Ed25519Group>::start_symmetric(&Password::new(password), &Identity::new(&id));
    println!("client - sending handshake msg");
    let handshake_msg = Message::HandshakeMessage(HandshakePayload {
        id: id.clone(),
        msg: Bytes::from(outbound_msg.clone()),
    });
    println!("client - handshake msg, {:?}", handshake_msg);
    println!(
        "len id: {:?}. len msg: {:?}",
        id.len(),
        Bytes::from(outbound_msg).len()
    );
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
    // println!("Handshake successful. Key is {:?}", key);
    return Ok((stream, new_cipher(&key)));
}

pub fn new_cipher(key: &Vec<u8>) -> Aes256Gcm {
    let key = Key::from_slice(&key[..]);
    Aes256Gcm::new(key)
}

pub const NONCE_SIZE_IN_BYTES: usize = 96 / 8;
pub fn encrypt(cipher: &Aes256Gcm, body: &Vec<u8>) -> Result<EncryptedPayload> {
    let mut arr = [0u8; NONCE_SIZE_IN_BYTES];
    thread_rng().try_fill(&mut arr[..])?;
    let nonce = Nonce::from_slice(&arr);
    let plaintext = body.as_ref();
    match cipher.encrypt(nonce, plaintext) {
        Ok(body) => Ok(EncryptedPayload {
            nonce: arr.to_vec(),
            body,
        }),
        Err(_) => Err(anyhow!("Encryption error")),
    }
}

pub fn decrypt(cipher: &Aes256Gcm, payload: &EncryptedPayload) -> Result<Bytes> {
    let nonce = Nonce::from_slice(payload.nonce.as_ref());
    match cipher.decrypt(nonce, payload.body.as_ref()) {
        Ok(payload) => Ok(Bytes::from(payload)),
        Err(_) => Err(anyhow!("Decryption error")),
    }
}
