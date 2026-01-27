use crate::conf::{HANDSHAKE_MSG_SIZE, ID_SIZE};

use anyhow::{anyhow, Result};
use blake2::{Blake2s256, Digest};
use bytes::{Bytes, BytesMut};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info};

pub struct Handshake {
    pub id: Bytes,
    pub outbound_msg: Bytes,
}

impl Handshake {
    pub fn from_password(pw: &String) -> Result<(Handshake, spake2::Spake2<spake2::Ed25519Group>)> {
        let password = Bytes::from(pw.clone());
        let id = Handshake::pass_to_bytes(&pw);
        let (s1, outbound_msg) =
            Spake2::<Ed25519Group>::start_symmetric(&Password::new(&password), &Identity::new(&id));
        let outbound_msg = Bytes::from(outbound_msg);
        let handshake = Handshake { id, outbound_msg };
        Ok((handshake, s1))
    }

    pub async fn from_socket(socket: TcpStream) -> Result<(Handshake, TcpStream)> {
        let mut socket = socket;
        let mut buffer = [0; ID_SIZE + HANDSHAKE_MSG_SIZE];
        let n = socket.read_exact(&mut buffer).await?;
        debug!(bytes_read = n, "Received handshake from client");
        let mut outbound_msg = BytesMut::from(&buffer[..n]).freeze();
        let id = outbound_msg.split_to(ID_SIZE);
        Ok((Handshake { id, outbound_msg }, socket))
    }

    pub fn to_bytes(self) -> Bytes {
        let mut buffer = BytesMut::with_capacity(ID_SIZE + HANDSHAKE_MSG_SIZE);
        buffer.extend_from_slice(&self.id);
        buffer.extend_from_slice(&self.outbound_msg);
        buffer.freeze()
    }

    pub async fn negotiate(
        self,
        socket: TcpStream,
        s1: spake2::Spake2<spake2::Ed25519Group>,
    ) -> Result<(TcpStream, Vec<u8>)> {
        let mut socket = socket;
        let bytes = &self.to_bytes();
        socket.write_all(&bytes).await?;
        let mut buffer = [0; HANDSHAKE_MSG_SIZE];
        let n = socket.read_exact(&mut buffer).await?;
        let response = BytesMut::from(&buffer[..n]).freeze();
        let key = match s1.finish(&response[..]) {
            Ok(key_bytes) => key_bytes,
            Err(e) => return Err(anyhow!(e.to_string())),
        };
        info!("Handshake successful");
        return Ok((socket, key));
    }

    fn pass_to_bytes(password: &String) -> Bytes {
        let bytes = Blake2s256::digest(password.as_bytes());
        BytesMut::from(&bytes[..]).freeze()
    }
}
