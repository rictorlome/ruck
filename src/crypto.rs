use crate::message::{HandshakePayload, Message, MessageStream};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures::prelude::*;
use spake2::{Ed25519Group, Identity, Password, Spake2};

pub async fn handshake(
    stream: &mut MessageStream,
    up: bool,
    password: Bytes,
    id: Bytes,
) -> Result<(&mut MessageStream, Bytes)> {
    let (s1, outbound_msg) =
        Spake2::<Ed25519Group>::start_symmetric(&Password::new(password), &Identity::new(&id));
    println!("client - sending handshake msg");
    let handshake_msg = Message::HandshakeMessage(HandshakePayload {
        up,
        id,
        msg: Bytes::from(outbound_msg),
    });
    println!("client - handshake msg, {:?}", handshake_msg);
    stream.send(handshake_msg).await?;
    let first_message = match stream.next().await {
        Some(Ok(msg)) => match msg {
            Message::HandshakeMessage(response) => response.msg,
            _ => return Err(anyhow!("Expecting handshake message response")),
        },
        _ => {
            return Err(anyhow!("No response to handshake message"));
        }
    };
    println!("client - handshake msg responded to");
    let key = match s1.finish(&first_message[..]) {
        Ok(key_bytes) => key_bytes,
        Err(e) => return Err(anyhow!(e.to_string())),
    };
    println!("Handshake successful. Key is {:?}", key);
    return Ok((stream, Bytes::from(key)));
}
