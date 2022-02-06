mod message;
use message::Message;

use bytes::Bytes;
use futures::prelude::*;
use tokio::net::TcpStream;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};

#[tokio::main]
pub async fn main() {
    // Bind a server socket
    let socket = TcpStream::connect("127.0.0.1:8080").await.unwrap();

    // Delimit frames using a length header
    let length_delimited = FramedWrite::new(socket, LengthDelimitedCodec::new());

    let m = Message {
        body: Bytes::from("hello world"),
    };
    let mut stream = tokio_serde::SymmetricallyFramed::new(
        length_delimited,
        tokio_serde::formats::SymmetricalBincode::<Message>::default(),
    );

    // Send the value
    stream.send(m).await.unwrap()
}
