use crate::message::EncryptedPayload;

use aes_gcm::aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Key, Nonce}; // Or `Aes128Gcm`
use anyhow::{anyhow, Result};
use bytes::{Bytes, BytesMut};
use rand::{thread_rng, Rng};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub async fn handshake(
    socket: TcpStream,
    password: Bytes,
    id: Bytes,
) -> Result<(TcpStream, Aes256Gcm)> {
    let mut socket = socket;
    let (s1, outbound_msg) =
        Spake2::<Ed25519Group>::start_symmetric(&Password::new(password), &Identity::new(&id));
    println!("client - sending handshake msg");
    let mut handshake_msg = BytesMut::with_capacity(32 + 33);
    handshake_msg.extend_from_slice(&id);
    handshake_msg.extend_from_slice(&outbound_msg);
    let handshake_msg = handshake_msg.freeze();
    println!("client - handshake msg, {:?}", handshake_msg);
    println!("id: {:?}. msg: {:?}", id.clone(), outbound_msg.clone());
    socket.write_all(&handshake_msg).await?;
    let mut buffer = [0; 33];
    let n = socket.read_exact(&mut buffer).await?;
    println!("The bytes: {:?}", &buffer[..n]);
    let first_message = BytesMut::from(&buffer[..n]).freeze();
    println!("client - handshake msg responded to: {:?}", first_message);
    let key = match s1.finish(&first_message[..]) {
        Ok(key_bytes) => key_bytes,
        Err(e) => return Err(anyhow!(e.to_string())),
    };
    println!("Handshake successful. Key is {:?}", key);
    return Ok((socket, new_cipher(&key)));
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
