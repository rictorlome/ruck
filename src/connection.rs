use crate::conf::BUFFER_SIZE;
use crate::crypto::Crypt;
use crate::file::{ChunkHeader, StdFileHandle};
use crate::message::{FileTransferPayload, Message, MessageStream};
use anyhow::{anyhow, Result};

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;

use bytes::{Bytes, BytesMut};
use flate2::bufread::GzEncoder;
use flate2::write::GzDecoder;
use flate2::Compression;
use std::io::{BufReader, Read, Write};

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

    pub async fn upload_file(&mut self, handle: StdFileHandle) -> Result<()> {
        let mut buffer = [0; BUFFER_SIZE];
        let reader = BufReader::new(handle.file);
        let mut gz = GzEncoder::new(reader, Compression::fast());
        loop {
            match gz.read(&mut buffer) {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    let message = Message::FileTransfer(FileTransferPayload {
                        chunk: BytesMut::from(&buffer[..n]).freeze(),
                        chunk_header: ChunkHeader {
                            id: handle.id,
                            start: 0,
                        },
                    });
                    self.send_msg(message).await?;
                }
                Err(e) => return Err(anyhow!(e.to_string())),
            }
        }
        Ok(())
    }

    pub async fn upload_files(mut self, handles: Vec<StdFileHandle>) -> Result<()> {
        for handle in handles {
            self.upload_file(handle).await?;
        }
        Ok(())
    }

    pub async fn download_files(mut self, handles: Vec<StdFileHandle>) -> Result<()> {
        for handle in handles {
            self.download_file(handle).await?;
        }
        Ok(())
    }

    pub async fn download_file(&mut self, handle: StdFileHandle) -> Result<()> {
        let mut decoder = GzDecoder::new(handle.file);
        loop {
            let msg = self.await_msg().await?;
            match msg {
                Message::FileTransfer(payload) => {
                    if payload.chunk_header.id != handle.id {
                        return Err(anyhow!("Wrong file"));
                    }
                    if payload.chunk.len() == 0 {
                        break;
                    }
                    decoder.write_all(&payload.chunk[..])?
                }
                _ => return Err(anyhow!("Expecting file transfer message")),
            }
        }
        decoder.finish()?;
        println!("Done downloading file.");
        Ok(())
    }
}
