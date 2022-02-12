use crate::crypto::handshake;
use crate::file::FileHandle;
use crate::message::{Message, MessageStream};

use anyhow::Result;
use blake2::{Blake2s256, Digest};
use bytes::{BufMut, Bytes, BytesMut};
use futures::future::join_all;
use futures::prelude::*;
use std::path::PathBuf;
use tokio::net::TcpStream;

fn pass_to_bytes(password: &String) -> Bytes {
    let mut hasher = Blake2s256::new();
    hasher.update(password.as_bytes());
    let res = hasher.finalize();
    BytesMut::from(&res[..]).freeze()
}

pub async fn send(file_paths: &Vec<PathBuf>, password: &String) -> Result<()> {
    let tasks = file_paths
        .into_iter()
        .map(|path| FileHandle::new(path.to_path_buf()).map(|f| f.map(|s| s.to_file_info())));
    let metadatas = join_all(tasks).await;
    println!("mds: {:?}", metadatas);
    return Ok(());

    let socket = TcpStream::connect("127.0.0.1:8080").await?;
    let mut stream = Message::to_stream(socket);

    let (stream, key) = handshake(
        &mut stream,
        true,
        Bytes::from(password.to_string()),
        pass_to_bytes(password),
    )
    .await?;

    // return upload_encrypted_files(stream, file_paths, key).await;
}

pub async fn receive(password: &String) -> Result<()> {
    let socket = TcpStream::connect("127.0.0.1:8080").await?;
    let mut stream = Message::to_stream(socket);
    let (stream, key) = handshake(
        &mut stream,
        false,
        Bytes::from(password.to_string()),
        pass_to_bytes(password),
    )
    .await?;
    return Ok(());
}

pub async fn upload_encrypted_files(
    stream: &mut MessageStream,
    file_paths: &Vec<PathBuf>,
    key: Bytes,
) -> Result<()> {
    return Ok(());
}
