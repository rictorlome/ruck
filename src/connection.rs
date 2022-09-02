use crate::conf::{BUFFER_SIZE, CHUNK_HEADER_SIZE};
use crate::crypto::Crypt;
use crate::file::{ChunkHeader, FileHandle, StdFileHandle};
use crate::message::{Message, MessageStream};
use anyhow::{anyhow, Result};

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;

use bytes::{Bytes, BytesMut};
use flate2::write::{GzDecoder, GzEncoder};
use flate2::Compression;
use std::io::{Read, Write};

pub struct Connection {
    ms: MessageStream,
    crypt: Crypt,
}

impl Connection {
    pub fn new(socket: TcpStream, key: Vec<u8>) -> Self {
        let ms = Message::to_stream(socket);
        let crypt = Crypt::new(&key);
        Connection { ms, crypt }
    }

    pub async fn send_bytes(&mut self, bytes: Bytes) -> Result<()> {
        match self.ms.send(bytes).await {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }

    pub async fn send_msg(&mut self, msg: Message) -> Result<()> {
        let msg = msg.serialize()?;
        let bytes = self.crypt.encrypt(msg)?;
        self.send_bytes(bytes).await
    }

    pub async fn await_msg(&mut self) -> Result<Message> {
        match self.ms.next().await {
            Some(Ok(msg)) => {
                let decrypted_bytes = self.crypt.decrypt(msg.freeze())?;
                Message::deserialize(decrypted_bytes)
            }
            Some(Err(e)) => Err(anyhow!(e.to_string())),
            None => Err(anyhow!("Error awaiting msg")),
        }
    }

    pub async fn upload_files(mut self, handles: Vec<StdFileHandle>) -> Result<()> {
        let mut socket = self.ms.into_inner().into_std()?;
        tokio::task::spawn_blocking(move || {
            for mut handle in handles {
                let mut buffer = BytesMut::with_capacity(BUFFER_SIZE);
                let mut start = handle.start;
                loop {
                    let end =
                        FileHandle::to_message(handle.id, &mut handle.file, &mut buffer, start)?;
                    let mut compressor = GzEncoder::new(Vec::new(), Compression::fast());
                    compressor.write_all(&buffer[..])?;
                    let compressed_bytes = compressor.finish()?;
                    let encrypted_bytes = self.crypt.encrypt(Bytes::from(compressed_bytes))?;
                    start = end;
                    socket.write(&encrypted_bytes[..])?;
                    if end == 0 {
                        break;
                    }
                }
            }
            Ok(())
        })
        .await?
    }

    pub async fn download_files(mut self, handles: Vec<StdFileHandle>) -> Result<()> {
        let mut socket = self.ms.into_inner().into_std()?;
        tokio::task::spawn_blocking(move || {
            for mut handle in handles {
                let mut buffer = BytesMut::with_capacity(BUFFER_SIZE);
                let mut start = handle.start;
                loop {
                    // read bytes
                    match socket.read(&mut buffer) {
                        Ok(0) => {
                            break;
                        }
                        Ok(n) => {
                            let decrypted_bytes =
                                self.crypt.decrypt(Bytes::from(&mut buffer[0..n]))?;
                            let mut writer = Vec::new();
                            let mut decompressor = GzDecoder::new(writer);
                            decompressor.write_all(&decrypted_bytes[..])?;
                            decompressor.try_finish()?;
                            writer = decompressor.finish()?;
                            let chunk_header: ChunkHeader =
                                bincode::deserialize(&writer[..CHUNK_HEADER_SIZE])?;
                            handle.file.write_all(&writer)
                        }
                        Err(e) => return Err(anyhow!(e.to_string())),
                    };
                }
            }
            Ok(())
        })
        .await?
    }
}
