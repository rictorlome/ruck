use crate::conf::NONCE_SIZE_IN_BYTES;
use aes_gcm::aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Key, Nonce}; // Or `Aes128Gcm`
use anyhow::{anyhow, Result};
use bytes::{Bytes, BytesMut};

use rand::{thread_rng, Rng};

pub struct Crypt {
    cipher: Aes256Gcm,
}

impl Crypt {
    pub fn new(key: &Vec<u8>) -> Crypt {
        let key = Key::from_slice(&key[..]);
        Crypt {
            cipher: Aes256Gcm::new(key),
        }
    }

    pub fn encrypt(&self, plaintext: Bytes) -> Result<Bytes> {
        let mut arr = [0u8; NONCE_SIZE_IN_BYTES];
        thread_rng().try_fill(&mut arr[..])?;
        let nonce = Nonce::from_slice(&arr);
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

    pub fn decrypt(&self, body: Bytes) -> Result<Bytes> {
        let mut body = body;
        let nonce_bytes = body.split_to(NONCE_SIZE_IN_BYTES);
        let nonce = Nonce::from_slice(&nonce_bytes);
        match self.cipher.decrypt(nonce, body.as_ref()) {
            Ok(payload) => Ok(Bytes::from(payload)),
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }
}
