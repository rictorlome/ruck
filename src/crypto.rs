use crate::conf::NONCE_SIZE_IN_BYTES;
use aes_gcm::aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Key, Nonce}; // Or `Aes128Gcm`
use anyhow::{anyhow, Result};
use bytes::{Bytes, BytesMut};

use rand::{thread_rng, Rng};

pub struct Crypt {
    cipher: Aes256Gcm,
    arr: [u8; NONCE_SIZE_IN_BYTES],
}

impl Crypt {
    pub fn new(key: &Vec<u8>) -> Crypt {
        let key = Key::from_slice(&key[..]);
        Crypt {
            cipher: Aes256Gcm::new(key),
            arr: [0u8; NONCE_SIZE_IN_BYTES],
        }
    }

    // Returns wire format, includes nonce as prefix
    pub fn encrypt(&mut self, plaintext: Bytes) -> Result<Bytes> {
        thread_rng().try_fill(&mut self.arr[..])?;
        let nonce = Nonce::from_slice(&self.arr);
        match self.cipher.encrypt(nonce, plaintext.as_ref()) {
            Ok(body) => {
                let mut buffer = BytesMut::with_capacity(NONCE_SIZE_IN_BYTES + body.len());
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
        let nonce_bytes = ciphertext_body.split_to(NONCE_SIZE_IN_BYTES);
        let nonce = Nonce::from_slice(&nonce_bytes);
        match self.cipher.decrypt(nonce, ciphertext_body.as_ref()) {
            Ok(payload) => Ok(Bytes::from(payload)),
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }
}
