use crate::conf::{HANDSHAKE_MSG_SIZE, ID_SIZE};
use crate::crypto::new_cipher;

use aes_gcm::Aes256Gcm; // Or `Aes128Gcm`
use anyhow::{anyhow, Result};
use blake2::{Blake2s256, Digest};
use bytes::{Bytes, BytesMut};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct Handshake {
    pub id: Bytes,
    pub outbound_msg: Bytes,
}

impl Handshake {
    pub fn from_password(pw: &String) -> (Handshake, spake2::Spake2<spake2::Ed25519Group>) {
        let password = Bytes::from(pw.to_string());
        let id = Handshake::pass_to_bytes(&pw);
        let (s1, outbound_msg) =
            Spake2::<Ed25519Group>::start_symmetric(&Password::new(&password), &Identity::new(&id));
        let mut buffer = BytesMut::with_capacity(HANDSHAKE_MSG_SIZE);
        buffer.extend_from_slice(&outbound_msg[..HANDSHAKE_MSG_SIZE]);
        let outbound_msg = buffer.freeze();
        let handshake = Handshake { id, outbound_msg };
        (handshake, s1)
    }

    pub async fn from_socket(socket: TcpStream) -> Result<(Handshake, TcpStream)> {
        let mut socket = socket;
        let mut buffer = BytesMut::with_capacity(ID_SIZE + HANDSHAKE_MSG_SIZE);
        match socket.read_exact(&mut buffer).await? {
            65 => Ok((Handshake::from_buffer(buffer), socket)), // magic number to catch correct capacity
            _ => return Err(anyhow!("invalid handshake buffer pulled from socket")),
        }
    }

    pub fn from_buffer(buffer: BytesMut) -> Handshake {
        let mut outbound_msg = BytesMut::from(&buffer[..ID_SIZE + HANDSHAKE_MSG_SIZE]).freeze();
        let id = outbound_msg.split_to(32);
        Handshake { id, outbound_msg }
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
    ) -> Result<(TcpStream, Aes256Gcm)> {
        let mut socket = socket;
        // println!("client - sending handshake msg");
        socket.write_all(&self.to_bytes()).await?;
        let mut buffer = [0; HANDSHAKE_MSG_SIZE];
        let n = socket.read_exact(&mut buffer).await?;
        let response = BytesMut::from(&buffer[..n]).freeze();
        // println!("client - handshake msg, {:?}", response);
        let key = match s1.finish(&response[..]) {
            Ok(key_bytes) => key_bytes,
            Err(e) => return Err(anyhow!(e.to_string())),
        };
        println!("Handshake successful. Key is {:?}", key);
        return Ok((socket, new_cipher(&key)));
    }

    fn pass_to_bytes(password: &String) -> Bytes {
        let mut hasher = Blake2s256::new();
        hasher.update(password.as_bytes());
        let res = hasher.finalize();
        BytesMut::from(&res[..]).freeze()
    }
}
