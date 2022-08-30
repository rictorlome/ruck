use crate::conf::NONCE_SIZE_IN_BYTES;
use crate::message::EncryptedPayload;
use aes_gcm::aead::{Aead, NewAead};
use aes_gcm::{Aes256Gcm, Key, Nonce}; // Or `Aes128Gcm`
use anyhow::{anyhow, Result};
use bytes::Bytes;

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

    pub fn encrypt(&self, body: &Vec<u8>) -> Result<EncryptedPayload> {
        let mut arr = [0u8; NONCE_SIZE_IN_BYTES];
        thread_rng().try_fill(&mut arr[..])?;
        let nonce = Nonce::from_slice(&arr);
        let plaintext = body.as_ref();
        match self.cipher.encrypt(nonce, plaintext) {
            Ok(body) => Ok(EncryptedPayload {
                nonce: arr.to_vec(),
                body,
            }),
            Err(_) => Err(anyhow!("Encryption error")),
        }
    }

    pub fn decrypt(&self, payload: &EncryptedPayload) -> Result<Bytes> {
        let nonce = Nonce::from_slice(payload.nonce.as_ref());
        match self.cipher.decrypt(nonce, payload.body.as_ref()) {
            Ok(payload) => Ok(Bytes::from(payload)),
            Err(_) => Err(anyhow!("Decryption error")),
        }
    }
}
