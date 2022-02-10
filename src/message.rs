use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_serde::{formats::SymmetricalBincode, SymmetricallyFramed};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    HandshakeMessage(HandshakeMessage),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandshakeMessage {
    pub id: Bytes,
    pub msg: Bytes,
}

impl Message {
    pub fn to_stream(stream: TcpStream) -> MessageStream {
        tokio_serde::SymmetricallyFramed::new(
            Framed::new(stream, LengthDelimitedCodec::new()),
            tokio_serde::formats::SymmetricalBincode::<Message>::default(),
        )
    }
}

pub type MessageStream = SymmetricallyFramed<
    Framed<TcpStream, LengthDelimitedCodec>,
    Message,
    SymmetricalBincode<Message>,
>;
