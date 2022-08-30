use crate::crypto::Crypt;
use crate::message::{Message, MessageStream};
use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;

pub struct Connection {
    ms: MessageStream,
    crypt: Crypt,
}

impl Connection {
    pub fn new(socket: TcpStream, key: Vec<u8>) -> Self {
        let ms = Message::to_stream(socket);
        let crypt = Crypt::new(&key);
        Connection { ms, crypt }
    }

    async fn send_bytes(&mut self, bytes: Bytes) -> Result<()> {
        match self.ms.send(bytes).await {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!(e.to_string())),
        }
    }

    pub async fn send_msg(&mut self, msg: Message) -> Result<()> {
        let msg = msg.serialize()?;
        let bytes = self.crypt.encrypt(msg)?;
        self.send_bytes(bytes).await
    }

    pub async fn await_msg(&mut self) -> Result<Message> {
        match self.ms.next().await {
            Some(Ok(msg)) => {
                let decrypted_bytes = self.crypt.decrypt(msg.freeze())?;
                Message::deserialize(decrypted_bytes)
            }
            Some(Err(e)) => Err(anyhow!(e.to_string())),
            None => Err(anyhow!("Error awaiting msg")),
        }
    }
}
