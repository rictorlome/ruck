use crate::file::FileInfo;

use anyhow::{anyhow, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
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

impl Message {
    pub fn serialize(&self) -> Result<Bytes> {
        match bincode::serialize(&self) {
            Ok(vec) => Ok(Bytes::from(vec)),
            Err(e) => Err(anyhow!(e.to_string())),
        }
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
