use crate::conf::{BUFFER_SIZE, ZSTD_COMPRESSION_LEVEL};
use crate::crypto::{Crypt, StreamCrypt};
use crate::file::StdFileHandle;
use crate::message::{
    should_compress, CompressionType, FileTransferStartPayload, Message, MessageStream,
};
use anyhow::{anyhow, Result};

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;

use async_compression::tokio::bufread::ZstdEncoder;
use async_compression::tokio::write::ZstdDecoder;
use bytes::{Bytes, BytesMut};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use colored::Colorize;

// Message type prefixes for wire protocol
const MSG_TYPE_CONTROL: u8 = 0x00;
const MSG_TYPE_DATA: u8 = 0x01;

pub struct Connection {
    ms: MessageStream,
    crypt: Crypt,
    key: Vec<u8>,
}

impl Connection {
    pub fn new(socket: TcpStream, key: Vec<u8>) -> Self {
        let ms = Message::to_stream(socket);
        let crypt = Crypt::new(&key);
        Connection { ms, crypt, key }
    }

    pub async fn send_bytes(&mut self, bytes: Bytes) -> Result<()> {
        match self.ms.send(bytes).await {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }

    // Send a control message (uses random nonce encryption)
    pub async fn send_msg(&mut self, msg: Message) -> Result<()> {
        let msg = msg.serialize()?;
        let encrypted = self.crypt.encrypt(msg)?;
        // Prefix with control message type
        let mut buffer = BytesMut::with_capacity(1 + encrypted.len());
        buffer.extend_from_slice(&[MSG_TYPE_CONTROL]);
        buffer.extend_from_slice(&encrypted);
        self.send_bytes(buffer.freeze()).await
    }

    // Send a raw data chunk (uses stream encryption, no message type overhead)
    async fn send_data_chunk(&mut self, chunk: Bytes) -> Result<()> {
        // Prefix with data message type
        let mut buffer = BytesMut::with_capacity(1 + chunk.len());
        buffer.extend_from_slice(&[MSG_TYPE_DATA]);
        buffer.extend_from_slice(&chunk);
        self.send_bytes(buffer.freeze()).await
    }

    // Await and parse a control message
    pub async fn await_msg(&mut self) -> Result<Message> {
        match self.ms.next().await {
            Some(Ok(msg)) => {
                let mut bytes = msg.freeze();
                if bytes.is_empty() {
                    return Err(anyhow!("Empty message received"));
                }
                let msg_type = bytes.split_to(1)[0];
                if msg_type != MSG_TYPE_CONTROL {
                    return Err(anyhow!("Expected control message, got data chunk"));
                }
                let decrypted_bytes = self.crypt.decrypt(bytes)?;
                Message::deserialize(decrypted_bytes)
            }
            Some(Err(e)) => Err(anyhow!(e.to_string())),
            None => Err(anyhow!("Error awaiting msg")),
        }
    }

    // Await raw bytes (for data chunks during transfer)
    async fn await_raw(&mut self) -> Result<(u8, Bytes)> {
        match self.ms.next().await {
            Some(Ok(msg)) => {
                let mut bytes = msg.freeze();
                if bytes.is_empty() {
                    return Err(anyhow!("Empty message received"));
                }
                let msg_type = bytes.split_to(1)[0];
                Ok((msg_type, bytes))
            }
            Some(Err(e)) => Err(anyhow!(e.to_string())),
            None => Err(anyhow!("Error awaiting raw bytes")),
        }
    }

    pub async fn upload_file(&mut self, handle: StdFileHandle) -> Result<()> {
        let before = Instant::now();

        // Generate session ID for this file transfer
        let session_id = StreamCrypt::generate_session_id();

        // Determine compression based on file type
        let use_compression = should_compress(&handle.name);
        let compression_type = if use_compression {
            CompressionType::Zstd
        } else {
            CompressionType::None
        };

        // Send FileTransferStart control message
        let start_msg = Message::FileTransferStart(FileTransferStartPayload {
            file_id: handle.id,
            session_id,
            compression: compression_type,
        });
        self.send_msg(start_msg).await?;

        // Create stream encryptor with the session ID
        let mut stream_crypt = StreamCrypt::new(&self.key, session_id);

        // Set up file reader
        let file = tokio::fs::File::from_std(handle.file);
        let reader = BufReader::new(file);

        // Set up progress bar based on original file size
        let pb = ProgressBar::new(handle.size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message(handle.name.clone());

        let mut buffer = vec![0u8; BUFFER_SIZE];

        let mut bytes_sent: u64 = 0;

        if use_compression {
            let mut encoder = ZstdEncoder::with_quality(reader, async_compression::Level::Precise(ZSTD_COMPRESSION_LEVEL));
            loop {
                let n = encoder.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }

                bytes_sent += n as u64;
                pb.set_position(bytes_sent.min(handle.size));

                let encrypted = stream_crypt.encrypt_chunk(&buffer[..n])?;
                self.send_data_chunk(encrypted).await?;
            }
        } else {
            let mut reader = reader;
            loop {
                let n = reader.read(&mut buffer).await?;
                if n == 0 {
                    break;
                }

                bytes_sent += n as u64;
                pb.set_position(bytes_sent);

                let encrypted = stream_crypt.encrypt_chunk(&buffer[..n])?;
                self.send_data_chunk(encrypted).await?;
            }
        }

        pb.finish_and_clear();

        // Send FileTransferComplete control message
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
        let (session_id, compression) = match start_msg {
            Message::FileTransferStart(payload) => {
                if payload.file_id != handle.id {
                    return Err(anyhow!(
                        "File ID mismatch: expected {}, got {}",
                        handle.id,
                        payload.file_id
                    ));
                }
                (payload.session_id, payload.compression)
            }
            _ => return Err(anyhow!("Expected FileTransferStart message")),
        };

        let use_compression = compression == CompressionType::Zstd;

        // Create stream decryptor with the session ID
        let mut stream_crypt = StreamCrypt::new(&self.key, session_id);

        // Set up progress bar based on expected file size
        let pb = ProgressBar::new(handle.size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );

        let file = tokio::fs::File::from_std(handle.file);
        let mut bytes_received: u64 = 0;

        if use_compression {
            let mut decoder = ZstdDecoder::new(file);

            loop {
                let (msg_type, bytes) = self.await_raw().await?;

                if msg_type == MSG_TYPE_CONTROL {
                    let decrypted = self.crypt.decrypt(bytes)?;
                    let msg = Message::deserialize(decrypted)?;
                    match msg {
                        Message::FileTransferComplete => break,
                        _ => return Err(anyhow!("Unexpected control message during transfer")),
                    }
                } else if msg_type == MSG_TYPE_DATA {
                    let decrypted = stream_crypt.decrypt_chunk(&bytes)?;
                    bytes_received += decrypted.len() as u64;
                    pb.set_position(bytes_received.min(handle.size));
                    decoder.write_all(&decrypted).await?;
                } else {
                    return Err(anyhow!("Unknown message type: {}", msg_type));
                }
            }

            decoder.shutdown().await?;
        } else {
            let mut file = file;

            loop {
                let (msg_type, bytes) = self.await_raw().await?;

                if msg_type == MSG_TYPE_CONTROL {
                    let decrypted = self.crypt.decrypt(bytes)?;
                    let msg = Message::deserialize(decrypted)?;
                    match msg {
                        Message::FileTransferComplete => break,
                        _ => return Err(anyhow!("Unexpected control message during transfer")),
                    }
                } else if msg_type == MSG_TYPE_DATA {
                    let decrypted = stream_crypt.decrypt_chunk(&bytes)?;
                    bytes_received += decrypted.len() as u64;
                    pb.set_position(bytes_received);
                    file.write_all(&decrypted).await?;
                } else {
                    return Err(anyhow!("Unknown message type: {}", msg_type));
                }
            }

            file.flush().await?;
        }

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
