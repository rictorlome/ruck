use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use tokio::net::TcpStream;
use tokio_serde::{formats::SymmetricalBincode, SymmetricallyFramed};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Message {
    HandshakeMessage(HandshakePayload),
    ErrorMessage(RuckError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandshakePayload {
    pub up: bool,
    pub id: Bytes,
    pub msg: Bytes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuckError {
    NotHandshake,
    SenderNotConnected,
    SenderAlreadyConnected,
    PairDisconnected,
}

impl fmt::Display for RuckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuckError is here!")
    }
}

impl Error for RuckError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self)
    }
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
