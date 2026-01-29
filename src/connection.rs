use crate::conf::{BUFFER_SIZE, ZSTD_COMPRESSION_LEVEL};
use crate::crypto::Crypt;
use crate::file::{should_compress, ChunkHeader, CompressionType, StdFileHandle};
use crate::message::{FileTransferPayload, FileTransferStartPayload, Message, MessageStream};

use anyhow::{anyhow, Result};
use async_compression::tokio::bufread::ZstdEncoder;
use async_compression::tokio::write::ZstdDecoder;
use bytes::{Bytes, BytesMut};
use colored::Colorize;
use futures::{SinkExt, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

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
        let encrypted = self.crypt.encrypt(msg)?;
        self.send_bytes(encrypted).await
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
        let before = Instant::now();

        // Determine compression based on file type
        let use_compression = should_compress(&handle.name);
        let compression_type = if use_compression {
            CompressionType::Zstd
        } else {
            CompressionType::None
        };

        // Send FileTransferStart message
        let start_msg = Message::FileTransferStart(FileTransferStartPayload {
            file_id: handle.id,
            compression: compression_type,
        });
        self.send_msg(start_msg).await?;

        // Set up file reader (already seeked to handle.start)
        let file = tokio::fs::File::from_std(handle.file);
        let reader = BufReader::new(file);

        // Set up progress bar for total file size, starting at resume position
        let pb = ProgressBar::new(handle.size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message(handle.name.clone());
        pb.set_position(handle.start);

        let mut buffer = vec![0u8; BUFFER_SIZE];
        let mut bytes_sent: u64 = 0;

        let mut reader: std::pin::Pin<Box<dyn tokio::io::AsyncRead + Send>> = if use_compression {
            Box::pin(ZstdEncoder::with_quality(reader, async_compression::Level::Precise(ZSTD_COMPRESSION_LEVEL)))
        } else {
            Box::pin(reader)
        };

        loop {
            let n = reader.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            bytes_sent += n as u64;
            pb.set_position((handle.start + bytes_sent).min(handle.size));

            let msg = Message::FileTransfer(FileTransferPayload {
                chunk_header: ChunkHeader {
                    id: handle.id,
                    start: handle.start,
                },
                chunk: BytesMut::from(&buffer[..n]).freeze(),
            });
            self.send_msg(msg).await?;
        }

        pb.finish_and_clear();

        // Send FileTransferComplete message
        self.send_msg(Message::FileTransferComplete).await?;

        let elapsed = before.elapsed();
        let mb_sent = bytes_sent as f64 / 1_048_576.0;
        let elapsed_secs = elapsed.as_secs_f64().max(0.001);
        println!(
            "{} {} ({:.1} MB, {:.1} MB/s)",
            "Sent".green(),
            handle.name,
            mb_sent,
            mb_sent / elapsed_secs
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
        let before = Instant::now();

        // Await FileTransferStart message
        let start_msg = self.await_msg().await?;
        let compression = match start_msg {
            Message::FileTransferStart(payload) => {
                if payload.file_id != handle.id {
                    return Err(anyhow!(
                        "File ID mismatch: expected {}, got {}",
                        handle.id,
                        payload.file_id
                    ));
                }
                payload.compression
            }
            _ => return Err(anyhow!("Expected FileTransferStart message")),
        };

        let use_compression = compression == CompressionType::Zstd;

        // Set up progress bar for total file size, starting at resume position
        let pb = ProgressBar::new(handle.size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_position(handle.start);

        let file = tokio::fs::File::from_std(handle.file);
        let mut bytes_received: u64 = 0;

        let mut writer: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send>> = if use_compression {
            Box::pin(ZstdDecoder::new(file))
        } else {
            Box::pin(file)
        };

        loop {
            let msg = self.await_msg().await?;
            match msg {
                Message::FileTransfer(payload) => {
                    if payload.chunk_header.id != handle.id {
                        return Err(anyhow!("File ID mismatch in chunk"));
                    }
                    bytes_received += payload.chunk.len() as u64;
                    pb.set_position((handle.start + bytes_received).min(handle.size));
                    writer.write_all(&payload.chunk).await?;
                }
                Message::FileTransferComplete => break,
                _ => return Err(anyhow!("Unexpected message during transfer")),
            }
        }

        writer.shutdown().await?;

        pb.finish_and_clear();

        let elapsed = before.elapsed();
        let mb_received = bytes_received as f64 / 1_048_576.0;
        let elapsed_secs = elapsed.as_secs_f64().max(0.001);
        println!(
            "{} {} ({:.1} MB, {:.1} MB/s)",
            "Received".green(),
            handle.name,
            mb_received,
            mb_received / elapsed_secs
        );

        // Verify file size
        let file = std::fs::File::open(&handle.name)?;
        Connection::check_and_finish_download(file, handle.name, handle.size).await?;
        Ok(())
    }

    pub async fn check_and_finish_download(
        file: std::fs::File,
        _filename: String,
        size: u64,
    ) -> Result<()> {
        let metadata = file.metadata()?;
        if metadata.len() == size {
            return Ok(());
        }
        return Err(anyhow!(
            "Downloaded file does not match expected size. Try again"
        ));
    }
}
