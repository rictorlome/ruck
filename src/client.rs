use crate::conf::BUFFER_SIZE;
use crate::connection::Connection;
use crate::file::{to_size_string, FileHandle, FileInfo};
use crate::handshake::Handshake;
use crate::message::{FileNegotiationPayload, FileTransferPayload, Message};

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

    let mut connection = Connection::new(socket, key);
    // Complete file negotiation
    let handles = negotiate_files_up(&mut connection, handles).await?;

    // Upload negotiated files
    upload_encrypted_files(&mut connection, handles).await?;
    println!("Done uploading.");

    // Exit
    Ok(())
}

pub async fn receive(password: &String) -> Result<()> {
    let socket = TcpStream::connect("127.0.0.1:8080").await?;
    let (handshake, s1) = Handshake::from_password(password);
    let (socket, key) = handshake.negotiate(socket, s1).await?;
    let mut connection = Connection::new(socket, key);
    let files = negotiate_files_down(&mut connection).await?;

    download_files(&mut connection, files).await?;
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
    conn: &mut Connection,
    file_handles: Vec<FileHandle>,
) -> Result<Vec<FileHandle>> {
    let files = file_handles.iter().map(|fh| fh.to_file_info()).collect();
    let msg = Message::FileNegotiationMessage(FileNegotiationPayload { files });
    conn.send_msg(msg).await?;
    let reply = conn.await_msg().await?;
    let requested_paths: Vec<PathBuf> = match reply {
        Message::FileNegotiationMessage(fnm) => fnm.files.into_iter().map(|f| f.path).collect(),
        _ => return Err(anyhow!("Expecting file negotiation message back")),
    };
    Ok(file_handles
        .into_iter()
        .filter(|fh| requested_paths.contains(&fh.path))
        .collect())
}

pub async fn negotiate_files_down(conn: &mut Connection) -> Result<Vec<FileInfo>> {
    let offer = conn.await_msg().await?;
    let requested_infos: Vec<FileInfo> = match offer {
        Message::FileNegotiationMessage(fnm) => fnm.files,
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
    let msg = Message::FileNegotiationMessage(FileNegotiationPayload {
        files: files.clone(),
    });
    conn.send_msg(msg).await?;
    Ok(files)
}

pub async fn upload_encrypted_files(conn: &mut Connection, handles: Vec<FileHandle>) -> Result<()> {
    for mut handle in handles {
        enqueue_file_chunks(conn, &mut handle).await?;
    }
    println!("Files uploaded.");
    Ok(())
}

pub async fn enqueue_file_chunks(conn: &mut Connection, fh: &mut FileHandle) -> Result<()> {
    let mut chunk_num = 0;
    let mut bytes_read = 1;
    while bytes_read != 0 {
        let mut buf = BytesMut::with_capacity(BUFFER_SIZE);
        bytes_read = fh.file.read_buf(&mut buf).await?;
        // println!("Bytes_read: {:?}, The bytes: {:?}", bytes_read, &buf[..]);
        if bytes_read != 0 {
            let chunk = buf.freeze();
            let file_info = fh.to_file_info();
            let ftp = Message::FileTransferMessage(FileTransferPayload {
                chunk,
                chunk_num,
                file_info,
            });
            conn.send_msg(ftp).await?;
            chunk_num += 1;
        }
    }

    Ok(())
}

pub async fn download_files(conn: &mut Connection, file_infos: Vec<FileInfo>) -> Result<()> {
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
            result = conn.await_msg() => match result {
                Ok(msg) => {
                    match msg {
                        Message::FileTransferMessage(payload) => {
                            if let Some(tx) = info_handles.get(&payload.file_info.path) {
                                tx.send((payload.chunk_num, payload.chunk))?
                            }
                        },
                        _ => {
                            println!("Wrong message type");
                            return Err(anyhow!("wrong message type"));
                        }
                    }
                },
                Err(e) => return Err(anyhow!(e.to_string())),
            }
        }
    }
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
