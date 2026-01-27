use crate::conf::SESSION_ID_SIZE;
use crate::file::{ChunkHeader, FileOffer};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CompressionType {
    None,
    Zstd,
}

/// Check if a file should be compressed based on extension
pub fn should_compress(filename: &str) -> bool {
    let skip_extensions = [
        // Images
        "jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "avif",
        // Video
        "mp4", "mkv", "avi", "mov", "webm", "m4v",
        // Audio
        "mp3", "aac", "ogg", "opus", "flac", "m4a",
        // Archives
        "zip", "gz", "bz2", "xz", "zst", "7z", "rar", "tar.gz", "tgz",
        // Other compressed formats
        "pdf", "docx", "xlsx", "pptx",
    ];

    let lower = filename.to_lowercase();
    !skip_extensions.iter().any(|ext| lower.ends_with(ext))
}

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
    pub session_id: [u8; SESSION_ID_SIZE],
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
