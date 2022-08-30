use crate::conf::BUFFER_SIZE;
use crate::crypto::Crypt;
use crate::file::{to_size_string, FileHandle, FileInfo};
use crate::handshake::Handshake;
use crate::message::{
    EncryptedMessage, FileNegotiationPayload, FileTransferPayload, Message, MessageStream,
};

use anyhow::{anyhow, Result};
use bytes::{Bytes, BytesMut};
use futures::future::try_join_all;
use futures::prelude::*;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, LinesCodec};

pub async fn send(file_paths: &Vec<PathBuf>, password: &String) -> Result<()> {
    // Fail early if there are problems generating file handles
    let handles = get_file_handles(file_paths).await?;

    // Establish connection to server
    let socket = TcpStream::connect("127.0.0.1:8080").await?;

    let (handshake, s1) = Handshake::from_password(password);
    // Complete handshake, returning key used for encryption
    let (socket, key) = handshake.negotiate(socket, s1).await?;

    let mut stream = Message::to_stream(socket);
    let crypt = Crypt::new(&key);
    // Complete file negotiation
    let handles = negotiate_files_up(handles, &mut stream, &crypt).await?;

    // Upload negotiated files
    upload_encrypted_files(&mut stream, handles, &crypt).await?;
    println!("Done uploading.");

    // Exit
    Ok(())
}

pub async fn receive(password: &String) -> Result<()> {
    let socket = TcpStream::connect("127.0.0.1:8080").await?;
    let (handshake, s1) = Handshake::from_password(password);
    let (socket, key) = handshake.negotiate(socket, s1).await?;
    let mut stream = Message::to_stream(socket);
    let crypt = Crypt::new(&key);
    let files = negotiate_files_down(&mut stream, &crypt).await?;

    download_files(files, &mut stream, &crypt).await?;
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
    crypt: &Crypt,
) -> Result<Vec<FileHandle>> {
    let files = file_handles.iter().map(|fh| fh.to_file_info()).collect();
    let msg = EncryptedMessage::FileNegotiationMessage(FileNegotiationPayload { files });
    let server_msg = msg.to_encrypted_message(crypt)?;
    println!("server_msg encrypted: {:?}", server_msg);
    stream.send(server_msg).await?;
    let reply_payload = match stream.next().await {
        Some(Ok(msg)) => match msg {
            Message::EncryptedMessage(response) => response,
        },
        _ => {
            return Err(anyhow!("No response to negotiation message"));
        }
    };
    let plaintext_reply = EncryptedMessage::from_encrypted_message(crypt, &reply_payload)?;
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

pub async fn negotiate_files_down(
    stream: &mut MessageStream,
    crypt: &Crypt,
) -> Result<Vec<FileInfo>> {
    let file_offer = match stream.next().await {
        Some(Ok(msg)) => match msg {
            Message::EncryptedMessage(response) => response,
        },
        _ => {
            return Err(anyhow!("No response to negotiation message"));
        }
    };
    let plaintext_offer = EncryptedMessage::from_encrypted_message(crypt, &file_offer)?;
    let requested_infos: Vec<FileInfo> = match plaintext_offer {
        EncryptedMessage::FileNegotiationMessage(fnm) => fnm.files,
        _ => return Err(anyhow!("Expecting file negotiation message back")),
    };
    let mut stdin = FramedRead::new(io::stdin(), LinesCodec::new());
    let mut files = vec![];
    for file_info in requested_infos.into_iter() {
        let mut reply = prompt_user_input(&mut stdin, &file_info).await;
        while reply.is_none() {
            reply = prompt_user_input(&mut stdin, &file_info).await;
        }
        match reply {
            Some(true) => files.push(file_info),
            _ => {}
        }
    }
    let msg = EncryptedMessage::FileNegotiationMessage(FileNegotiationPayload {
        files: files.clone(),
    });
    let server_msg = msg.to_encrypted_message(crypt)?;
    stream.send(server_msg).await?;
    Ok(files)
}

pub async fn upload_encrypted_files(
    stream: &mut MessageStream,
    handles: Vec<FileHandle>,
    cipher: &Crypt,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<EncryptedMessage>();
    for mut handle in handles {
        let txc = tx.clone();
        tokio::spawn(async move {
            let _ = enqueue_file_chunks(&mut handle, txc).await;
        });
    }

    loop {
        tokio::select! {
            Some(msg) = rx.recv() => {
                // println!("message received to client.rx {:?}", msg);
                let x = msg.to_encrypted_message(cipher)?;
                stream.send(x).await?
            }
            else => {
                println!("breaking");
                break
            },
        }
    }
    Ok(())
}

pub async fn enqueue_file_chunks(
    fh: &mut FileHandle,
    tx: mpsc::UnboundedSender<EncryptedMessage>,
) -> Result<()> {
    let mut chunk_num = 0;
    let mut bytes_read = 1;
    while bytes_read != 0 {
        let mut buf = BytesMut::with_capacity(BUFFER_SIZE);
        bytes_read = fh.file.read_buf(&mut buf).await?;
        // println!("Bytes_read: {:?}, The bytes: {:?}", bytes_read, &buf[..]);
        if bytes_read != 0 {
            let chunk = buf.freeze();
            let file_info = fh.to_file_info();
            let ftp = EncryptedMessage::FileTransferMessage(FileTransferPayload {
                chunk,
                chunk_num,
                file_info,
            });
            tx.send(ftp)?;
            chunk_num += 1;
        }
    }

    Ok(())
}

pub async fn download_files(
    file_infos: Vec<FileInfo>,
    stream: &mut MessageStream,
    cipher: &Crypt,
) -> Result<()> {
    // for each file_info
    let mut info_handles: HashMap<PathBuf, mpsc::UnboundedSender<(u64, Bytes)>> = HashMap::new();
    for fi in file_infos {
        let (tx, rx) = mpsc::unbounded_channel::<(u64, Bytes)>();
        let path = fi.path.clone();
        tokio::spawn(async move { download_file(fi, rx).await });
        info_handles.insert(path, tx);
    }
    loop {
        tokio::select! {
            result = stream.next() => match result {
                Some(Ok(Message::EncryptedMessage(payload))) => {
                    let ec = EncryptedMessage::from_encrypted_message(cipher, &payload)?;
                    // println!("encrypted message received! {:?}", ec);
                    match ec {
                        EncryptedMessage::FileTransferMessage(payload) => {
                            // println!("matched file transfer message");
                            if let Some(tx) = info_handles.get(&payload.file_info.path) {
                                // println!("matched on filetype, sending to tx");
                                tx.send((payload.chunk_num, payload.chunk))?
                            };
                        },
                        _ => {println!("wrong msg")}
                    }
                }
                Some(Err(e)) => {
                    println!("Error {:?}", e);
                }
                None => break,
            }
        }
    }
    Ok(())
}

pub async fn download_file(
    file_info: FileInfo,
    rx: mpsc::UnboundedReceiver<(u64, Bytes)>,
) -> Result<()> {
    println!("in download file");
    let mut rx = rx;
    let filename = match file_info.path.file_name() {
        Some(f) => {
            println!("matched filename");
            f
        }
        None => {
            println!("didnt match filename");
            OsStr::new("random.txt")
        }
    };
    println!("trying to create file...filename={:?}", filename);
    let mut file = File::create(filename).await?;
    println!("file created ok! filename={:?}", filename);
    while let Some((_chunk_num, chunk)) = rx.recv().await {
        // println!("rx got message! chunk={:?}", chunk);
        file.write_all(&chunk).await?;
    }
    println!("done receiving messages");
    Ok(())
}

pub async fn prompt_user_input(
    stdin: &mut FramedRead<io::Stdin, LinesCodec>,
    file_info: &FileInfo,
) -> Option<bool> {
    let prompt_name = file_info.path.file_name().unwrap();
    println!(
        "Accept {:?}? ({:?}). (Y/n)",
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
