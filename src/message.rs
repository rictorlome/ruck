use crate::crypto::Crypt;
use crate::file::FileInfo;

use anyhow::{anyhow, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use tokio::net::TcpStream;
use tokio_serde::{formats::SymmetricalBincode, SymmetricallyFramed};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    EncryptedMessage(EncryptedPayload),
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
    pub fn from_encrypted_message(crypt: &Crypt, payload: &EncryptedPayload) -> Result<Self> {
        let raw = crypt.decrypt(payload)?;
        let res = match bincode::deserialize(raw.as_ref()) {
            Ok(result) => result,
            Err(e) => {
                println!("deserialize error {:?}", e);
                return Err(anyhow!("deser error"));
            }
        };
        Ok(res)
    }
    pub fn to_encrypted_message(&self, crypt: &Crypt) -> Result<Message> {
        let raw = match bincode::serialize(&self) {
            Ok(result) => result,
            Err(e) => {
                println!("serialize error {:?}", e);
                return Err(anyhow!("serialize error"));
            }
        };
        let payload = crypt.encrypt(&raw)?;
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
