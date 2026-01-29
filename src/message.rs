use crate::file::{ChunkHeader, CompressionType, FileOffer};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    FileOffer(FileOfferPayload),
    FileRequest(FileRequestPayload),
    FileTransferStart(FileTransferStartPayload),
    FileTransfer(FileTransferPayload),
    FileTransferComplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileTransferStartPayload {
    pub file_id: u8,
    pub compression: CompressionType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileRequestPayload {
    pub chunks: Vec<ChunkHeader>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileOfferPayload {
    pub files: Vec<FileOffer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileTransferPayload {
    pub chunk_header: ChunkHeader,
    pub chunk: Bytes,
}

impl Message {
    pub fn serialize(&self) -> Result<Bytes> {
        bincode::serialize(&self).map(|vec| Ok(Bytes::from(vec)))?
    }
    pub fn deserialize(bytes: Bytes) -> Result<Self> {
        match bincode::deserialize(bytes.as_ref()) {
            Ok(msg) => Ok(msg),
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }
}

impl Message {
    pub fn to_stream(stream: TcpStream) -> MessageStream {
        Framed::new(stream, LengthDelimitedCodec::new())
    }
}

pub type MessageStream = Framed<TcpStream, LengthDelimitedCodec>;
