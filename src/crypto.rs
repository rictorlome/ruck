use crate::conf::{NONCE_SIZE, SESSION_ID_SIZE};
use aes_gcm::aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Key, Nonce}; // Or `Aes128Gcm`
use anyhow::{anyhow, Result};
use bytes::{Bytes, BytesMut};

use rand::{thread_rng, Rng};

pub struct Crypt {
    cipher: Aes256Gcm,
    arr: [u8; NONCE_SIZE],
}

impl Crypt {
    pub fn new(key: &Vec<u8>) -> Crypt {
        let key = Key::from_slice(&key[..]);
        Crypt {
            cipher: Aes256Gcm::new(key),
            arr: [0u8; NONCE_SIZE],
        }
    }

    // Returns wire format, includes nonce as prefix
    pub fn encrypt(&mut self, plaintext: Bytes) -> Result<Bytes> {
        thread_rng().try_fill(&mut self.arr[..])?;
        let nonce = Nonce::from_slice(&self.arr);
        match self.cipher.encrypt(nonce, plaintext.as_ref()) {
            Ok(body) => {
                let mut buffer = BytesMut::with_capacity(NONCE_SIZE + body.len());
                buffer.extend_from_slice(nonce);
                buffer.extend_from_slice(&body);
                Ok(buffer.freeze())
            }
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }

    // Accepts wire format, includes nonce as prefix
    pub fn decrypt(&self, ciphertext: Bytes) -> Result<Bytes> {
        let mut ciphertext_body = ciphertext;
        let nonce_bytes = ciphertext_body.split_to(NONCE_SIZE);
        let nonce = Nonce::from_slice(&nonce_bytes);
        match self.cipher.decrypt(nonce, ciphertext_body.as_ref()) {
            Ok(payload) => Ok(Bytes::from(payload)),
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }
}

/// StreamCrypt provides counter-based nonce encryption for streaming data.
/// Nonce format: [4-byte counter || 8-byte session_id]
/// Counter increments per chunk (no random generation overhead).
/// Session ID is randomized per file transfer for uniqueness.
pub struct StreamCrypt {
    cipher: Aes256Gcm,
    counter: u32,
    session_id: [u8; SESSION_ID_SIZE],
}

impl StreamCrypt {
    pub fn new(key: &Vec<u8>, session_id: [u8; SESSION_ID_SIZE]) -> StreamCrypt {
        let key = Key::from_slice(&key[..]);
        StreamCrypt {
            cipher: Aes256Gcm::new(key),
            counter: 0,
            session_id,
        }
    }

    /// Generate a new random session ID for a file transfer
    pub fn generate_session_id() -> [u8; SESSION_ID_SIZE] {
        let mut session_id = [0u8; SESSION_ID_SIZE];
        thread_rng().fill(&mut session_id);
        session_id
    }

    /// Build nonce from counter and session_id
    /// Format: [4-byte counter || 8-byte session_id] = 12 bytes
    fn build_nonce(&self) -> [u8; NONCE_SIZE] {
        let mut nonce = [0u8; NONCE_SIZE];
        nonce[..4].copy_from_slice(&self.counter.to_le_bytes());
        nonce[4..].copy_from_slice(&self.session_id);
        nonce
    }

    /// Encrypt a chunk and increment the counter.
    /// Returns only the ciphertext (nonce is deterministic from counter).
    pub fn encrypt_chunk(&mut self, plaintext: &[u8]) -> Result<Bytes> {
        let nonce_bytes = self.build_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        match self.cipher.encrypt(nonce, plaintext) {
            Ok(ciphertext) => {
                self.counter += 1;
                Ok(Bytes::from(ciphertext))
            }
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }

    /// Decrypt a chunk and increment the counter.
    /// Expects only ciphertext (nonce is deterministic from counter).
    pub fn decrypt_chunk(&mut self, ciphertext: &[u8]) -> Result<Bytes> {
        let nonce_bytes = self.build_nonce();
        let nonce = Nonce::from_slice(&nonce_bytes);
        match self.cipher.decrypt(nonce, ciphertext) {
            Ok(plaintext) => {
                self.counter += 1;
                Ok(Bytes::from(plaintext))
            }
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }
}
