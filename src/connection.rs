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
use std::time::Instant;

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
        let before = Instant::now();
        let mut count = 0;
        let mut bytes_sent: u64 = 0;
        loop {
            count += 1;
            match gz.read(&mut buffer) {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    bytes_sent += n as u64;
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
        self.send_msg(Message::FileTransferComplete).await?;
        let elapsed = before.elapsed();
        let mb_sent = bytes_sent / 1_048_576;
        println!(
            "{:?}: {:?} mb sent (compressed), {:?} messages. {:?} total time, {:?} avg per msg, {:?} avg mb/sec",
            handle.name,
            mb_sent,
            count,
            elapsed,
            elapsed / count,
            1000 * mb_sent as u128 / elapsed.as_millis()
        );
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
        let clone = handle.file.try_clone()?;
        let mut decoder = GzDecoder::new(handle.file);
        loop {
            let msg = self.await_msg().await?;
            match msg {
                Message::FileTransfer(payload) => {
                    if payload.chunk_header.id != handle.id {
                        return Err(anyhow!("Wrong file"));
                    }
                    decoder.write_all(&payload.chunk[..])?
                }
                Message::FileTransferComplete => {
                    break;
                }
                _ => return Err(anyhow!("Expecting file transfer message")),
            }
        }
        decoder.finish()?;
        println!("Done downloading {:?}.", handle.name);
        Connection::check_and_finish_download(clone, handle.name, handle.size).await?;
        Ok(())
    }

    pub async fn check_and_finish_download(
        file: std::fs::File,
        filename: String,
        size: u64,
    ) -> Result<()> {
        let metadata = file.metadata()?;
        if metadata.len() == size {
            println!("OK: downloaded {:?} matches advertised size.", filename);
            return Ok(());
        }
        return Err(anyhow!(
            "Downloaded file does not match expected size. Try again"
        ));
    }
}
