use crate::crypto::handshake;
use crate::message::{Message, MessageStream};

use anyhow::Result;
use blake2::{Blake2s256, Digest};
use bytes::{BufMut, Bytes, BytesMut};
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
    let socket = TcpStream::connect("127.0.0.1:8080").await?;
    let mut stream = Message::to_stream(socket);

    let (stream, key) = handshake(
        &mut stream,
        true,
        Bytes::from(password.to_string()),
        pass_to_bytes(password),
    )
    .await?;

    return upload_encrypted_files(stream, file_paths, key).await;

    // Send the value
    // for path in paths.iter() {
    //     let b = path.to_str().unwrap().as_bytes();
    //     let mut buf = BytesMut::with_capacity(1024);
    //     buf.put(&b[..]);
    //     let body = buf.freeze();
    //     let m = Message {
    //         key: "abc".to_string(),
    //         from_sender: true,
    //         body: body,
    //     };
    //     stream.send(m).await.unwrap();
    // }
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
