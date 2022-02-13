use crate::crypto::handshake;
use crate::file::{to_size_string, FileHandle, FileInfo};
use crate::message::{
    EncryptedMessage, FileNegotiationPayload, FileTransferPayload, Message, MessageStream,
};

use aes_gcm::Aes256Gcm;
use anyhow::{anyhow, Result};
use blake2::{Blake2s256, Digest};
use bytes::{Bytes, BytesMut};
use futures::future::try_join_all;
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::io::{self, AsyncReadExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};

fn pass_to_bytes(password: &String) -> Bytes {
    let mut hasher = Blake2s256::new();
    hasher.update(password.as_bytes());
    let res = hasher.finalize();
    BytesMut::from(&res[..]).freeze()
}

pub async fn send(file_paths: &Vec<PathBuf>, password: &String) -> Result<()> {
    // Fail early if there are problems generating file handles
    let handles = get_file_handles(file_paths).await?;

    // Establish connection to server
    let socket = TcpStream::connect("127.0.0.1:8080").await?;
    let mut stream = Message::to_stream(socket);

    // Complete handshake, returning cipher used for encryption
    let (stream, cipher) = handshake(
        &mut stream,
        true,
        Bytes::from(password.to_string()),
        pass_to_bytes(password),
    )
    .await?;

    // Complete file negotiation
    let handles = negotiate_files_up(handles, stream, &cipher).await?;

    // Upload negotiated files

    // Exit
    Ok(())
}

pub async fn receive(password: &String) -> Result<()> {
    let socket = TcpStream::connect("127.0.0.1:8080").await?;
    let mut stream = Message::to_stream(socket);
    let (stream, cipher) = handshake(
        &mut stream,
        false,
        Bytes::from(password.to_string()),
        pass_to_bytes(password),
    )
    .await?;

    let files = negotiate_files_down(stream, &cipher).await?;
    return Ok(());
}

pub async fn get_file_handles(file_paths: &Vec<PathBuf>) -> Result<Vec<FileHandle>> {
    let tasks = file_paths
        .into_iter()
        .map(|path| FileHandle::new(path.to_path_buf()));
    let handles = try_join_all(tasks).await?;
    Ok(handles)
}

pub async fn negotiate_files_up(
    file_handles: Vec<FileHandle>,
    stream: &mut MessageStream,
    cipher: &Aes256Gcm,
) -> Result<Vec<FileHandle>> {
    let files = file_handles.iter().map(|fh| fh.to_file_info()).collect();
    let msg = EncryptedMessage::FileNegotiationMessage(FileNegotiationPayload { files });
    let server_msg = msg.to_encrypted_message(cipher)?;
    stream.send(server_msg).await?;
    let reply_payload = match stream.next().await {
        Some(Ok(msg)) => match msg {
            Message::EncryptedMessage(response) => response,
            _ => return Err(anyhow!("Expecting encrypted message back")),
        },
        _ => {
            return Err(anyhow!("No response to negotiation message"));
        }
    };
    let plaintext_reply = EncryptedMessage::from_encrypted_message(cipher, &reply_payload)?;
    let requested_paths: Vec<PathBuf> = match plaintext_reply {
        EncryptedMessage::FileNegotiationMessage(fnm) => {
            fnm.files.into_iter().map(|f| f.path).collect()
        }
        _ => return Err(anyhow!("Expecting file negotiation message back")),
    };
    Ok(file_handles
        .into_iter()
        .filter(|fh| requested_paths.contains(&fh.path))
        .collect())
}

pub async fn negotiate_files_down(stream: &mut MessageStream, cipher: &Aes256Gcm) -> Result<()> {
    let file_offer = match stream.next().await {
        Some(Ok(msg)) => match msg {
            Message::EncryptedMessage(response) => response,
            _ => return Err(anyhow!("Expecting encrypted message back")),
        },
        _ => {
            return Err(anyhow!("No response to negotiation message"));
        }
    };
    let plaintext_offer = EncryptedMessage::from_encrypted_message(cipher, &file_offer)?;
    let requested_infos: Vec<FileInfo> = match plaintext_offer {
        EncryptedMessage::FileNegotiationMessage(fnm) => fnm.files,
        _ => return Err(anyhow!("Expecting file negotiation message back")),
    };
    let mut stdin = FramedRead::new(io::stdin(), LinesCodec::new());
    let mut files = vec![];
    for path in requested_infos.into_iter() {
        let mut reply = prompt_user_input(&mut stdin, &path).await;
        while reply.is_none() {
            reply = prompt_user_input(&mut stdin, &path).await;
        }
        match reply {
            Some(true) => files.push(path),
            _ => {}
        }
    }
    let msg = EncryptedMessage::FileNegotiationMessage(FileNegotiationPayload { files });
    let server_msg = msg.to_encrypted_message(cipher)?;
    stream.send(server_msg).await?;
    Ok(())
}

pub async fn upload_encrypted_files(
    stream: &mut MessageStream,
    handles: Vec<FileHandle>,
    cipher: &Aes256Gcm,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<EncryptedMessage>();
    //turn foo into something more concrete
    for mut handle in handles {
        let txc = tx.clone();
        tokio::spawn(async move {
            let _ = enqueue_file_chunks(&mut handle, txc).await;
        });
    }

    loop {
        tokio::select! {
            Some(msg) = rx.recv() => {
                println!("message received to client.rx {:?}", msg);
                let x = msg.to_encrypted_message(cipher)?;
                stream.send(x).await?
            }
            else => break,
        }
    }
    Ok(())
}
const BUFFER_SIZE: usize = 1024 * 64;
pub async fn enqueue_file_chunks(
    fh: &mut FileHandle,
    tx: mpsc::UnboundedSender<EncryptedMessage>,
) -> Result<()> {
    // let mut buf = BytesMut::with_capacity(BUFFER_SIZE);

    // // The `read` method is defined by this trait.
    // let mut chunk_num = 0;
    // while {
    //     let n = fh.file.read(&mut buf[..]).await?;
    //     n == 0
    // } {
    //     let chunk = buf.freeze();
    //     let file_info = fh.to_file_info();
    //     let ftp = EncryptedMessage::FileTransferMessage(FileTransferPayload {
    //         chunk,
    //         chunk_num,
    //         file_info,
    //     });
    //     tx.send(ftp);
    //     chunk_num += 1;
    // }

    Ok(())
}

pub async fn prompt_user_input(
    stdin: &mut FramedRead<io::Stdin, LinesCodec>,
    file_info: &FileInfo,
) -> Option<bool> {
    let prompt_name = file_info.path.file_name().unwrap();
    println!(
        "Do you want to download {:?}? It's {:?}. (Y/n)",
        prompt_name,
        to_size_string(file_info.size)
    );
    match stdin.next().await {
        Some(Ok(line)) => match line.as_str() {
            "" | "Y" | "y" | "yes" | "Yes" | "YES" => Some(true),
            "N" | "n" | "NO" | "no" | "No" => Some(false),
            _ => {
                println!("Invalid input. Please enter one of the following characters: [YyNn]");
                return None;
            }
        },
        _ => None,
    }
}
