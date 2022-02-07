use crate::message::Message;

use bytes::{BufMut, BytesMut};
use futures::prelude::*;
use std::path::PathBuf;
use tokio::net::TcpStream;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};

pub async fn send(paths: &Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    // Delimit frames using a length header
    let socket = TcpStream::connect("127.0.0.1:8080").await.unwrap();
    let length_delimited = FramedWrite::new(socket, LengthDelimitedCodec::new());
    let mut stream = tokio_serde::SymmetricallyFramed::new(
        length_delimited,
        tokio_serde::formats::SymmetricalBincode::<Message>::default(),
    );

    // Send the value
    for path in paths.iter() {
        let b = path.to_str().unwrap().as_bytes();
        let mut buf = BytesMut::with_capacity(1024);
        buf.put(&b[..]);
        let body = buf.freeze();
        let m = Message { body: body };
        stream.send(m).await.unwrap();
    }
    Ok(())
}
