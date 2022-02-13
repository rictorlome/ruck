use crate::crypto::{decrypt, encrypt};
use crate::file::FileInfo;

use aes_gcm::Aes256Gcm; // Or `Aes128Gcm`
use anyhow::{anyhow, Result};
use bincode::config;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use tokio::net::TcpStream;
use tokio_serde::{formats::SymmetricalBincode, SymmetricallyFramed};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    HandshakeMessage(HandshakePayload),
    EncryptedMessage(EncryptedPayload),
    ErrorMessage(RuckError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandshakePayload {
    pub up: bool,
    pub id: Bytes,
    pub msg: Bytes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub nonce: Vec<u8>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EncryptedMessage {
    FileNegotiationMessage(FileNegotiationPayload),
    FileTransferMessage(FileTransferPayload),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileNegotiationPayload {
    pub files: Vec<FileInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileTransferPayload {
    pub file_info: FileInfo,
    pub chunk_num: u64,
    pub chunk: Bytes,
}

impl EncryptedMessage {
    pub fn from_encrypted_message(cipher: &Aes256Gcm, payload: &EncryptedPayload) -> Result<Self> {
        let raw = decrypt(cipher, payload)?;
        let res = match bincode::deserialize(raw.as_ref()) {
            Ok(result) => result,
            Err(e) => {
                println!("deserialize error {:?}", e);
                return Err(anyhow!("deser error"));
            }
        };
        Ok(res)
    }
    pub fn to_encrypted_message(&self, cipher: &Aes256Gcm) -> Result<Message> {
        let raw = match bincode::serialize(&self) {
            Ok(result) => result,
            Err(e) => {
                println!("serialize error {:?}", e);
                return Err(anyhow!("serialize error"));
            }
        };
        let payload = encrypt(cipher, &raw)?;
        Ok(Message::EncryptedMessage(payload))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuckError {
    NotHandshake,
    SenderNotConnected,
    SenderAlreadyConnected,
    PairDisconnected,
}

impl fmt::Display for RuckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuckError is here!")
    }
}

impl Error for RuckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self)
    }
}

impl Message {
    pub fn to_stream(stream: TcpStream) -> MessageStream {
        tokio_serde::SymmetricallyFramed::new(
            Framed::new(stream, LengthDelimitedCodec::new()),
            tokio_serde::formats::SymmetricalBincode::<Message>::default(),
        )
    }
}

pub type MessageStream = SymmetricallyFramed<
    Framed<TcpStream, LengthDelimitedCodec>,
    Message,
    SymmetricalBincode<Message>,
>;
