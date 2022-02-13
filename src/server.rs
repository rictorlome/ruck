use crate::message::{HandshakePayload, Message, MessageStream, RuckError};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures::prelude::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};

type Tx = mpsc::UnboundedSender<Message>;
type Rx = mpsc::UnboundedReceiver<Message>;

pub struct Shared {
    handshakes: HashMap<Bytes, Rx>,
    senders: HashMap<Bytes, Tx>,
    receivers: HashMap<Bytes, Tx>,
}
type State = Arc<Mutex<Shared>>;

struct Client<'a> {
    up: bool,
    id: Bytes,
    messages: &'a mut MessageStream,
    rx: Rx,
}

impl Shared {
    fn new() -> Self {
        Shared {
            handshakes: HashMap::new(),
            senders: HashMap::new(),
            receivers: HashMap::new(),
        }
    }
    async fn relay<'a>(&self, client: &Client<'a>, message: Message) -> Result<()> {
        println!("in relay - got client={:?}, msg {:?}", client.id, message);
        match client.up {
            true => match self.receivers.get(&client.id) {
                Some(tx) => {
                    tx.send(message)?;
                }
                None => {
                    return Err(anyhow!(RuckError::PairDisconnected));
                }
            },
            false => match self.senders.get(&client.id) {
                Some(tx) => {
                    tx.send(message)?;
                }
                None => {
                    return Err(anyhow!(RuckError::PairDisconnected));
                }
            },
        }
        Ok(())
    }
}

impl<'a> Client<'a> {
    async fn new(
        up: bool,
        id: Bytes,
        state: State,
        messages: &'a mut MessageStream,
    ) -> Result<Client<'a>> {
        let (tx, rx) = mpsc::unbounded_channel();
        println!("server - creating client up={:?}, id={:?}", up, id);
        let shared = &mut state.lock().await;
        match shared.senders.get(&id) {
            Some(_) if up => {
                messages
                    .send(Message::ErrorMessage(RuckError::SenderAlreadyConnected))
                    .await?;
            }
            Some(_) => {
                println!("server - adding client to receivers");
                shared.receivers.insert(id.clone(), tx);
            }
            None if up => {
                println!("server - adding client to senders");
                shared.senders.insert(id.clone(), tx);
            }
            None => {
                messages
                    .send(Message::ErrorMessage(RuckError::SenderNotConnected))
                    .await?;
            }
        }
        Ok(Client {
            up,
            id,
            messages,
            rx,
        })
    }
    async fn complete_handshake(&mut self, state: State, msg: Message) -> Result<()> {
        match self.up {
            true => {
                let (tx, rx) = mpsc::unbounded_channel();
                tx.send(msg)?;
                state.lock().await.handshakes.insert(self.id.clone(), rx);
            }
            false => {
                let shared = &mut state.lock().await;
                if let Some(tx) = shared.senders.get(&self.id) {
                    tx.send(msg)?;
                }
                if let Some(mut rx) = shared.handshakes.remove(&self.id) {
                    drop(shared);
                    if let Some(msg) = rx.recv().await {
                        self.messages.send(msg).await?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub async fn serve() -> Result<()> {
    let addr = "127.0.0.1:8080".to_string();
    let listener = TcpListener::bind(&addr).await?;
    let state = Arc::new(Mutex::new(Shared::new()));
    println!("Listening on: {}", addr);
    loop {
        let (stream, address) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            match handle_connection(state, stream, address).await {
                Ok(_) => println!("Connection complete!"),
                Err(err) => println!("Error handling connection! {:?}", err),
            }
        });
    }
}

pub async fn handle_connection(
    state: Arc<Mutex<Shared>>,
    socket: TcpStream,
    addr: SocketAddr,
) -> Result<()> {
    let mut stream = Message::to_stream(socket);
    println!("server - new conn from {:?}", addr);
    let handshake_payload = match stream.next().await {
        Some(Ok(Message::HandshakeMessage(payload))) => payload,
        Some(Ok(_)) => {
            stream
                .send(Message::ErrorMessage(RuckError::NotHandshake))
                .await?;
            return Ok(());
        }
        _ => {
            println!("No first message");
            return Ok(());
        }
    };
    println!("server - received msg from {:?}", addr);
    let mut client = Client::new(
        handshake_payload.up,
        handshake_payload.id.clone(),
        state.clone(),
        &mut stream,
    )
    .await?;
    client
        .complete_handshake(state.clone(), Message::HandshakeMessage(handshake_payload))
        .await?;
    // add client to state here
    loop {
        tokio::select! {
            Some(msg) = client.rx.recv() => {
                println!("message received to client.rx {:?}", msg);
                client.messages.send(msg).await?
            }
            result = client.messages.next() => match result {
                Some(Ok(msg)) => {
                    println!("GOT: {:?}", msg);
                    let state = state.lock().await;
                    state.relay(&client, msg).await?;
                }
                Some(Err(e)) => {
                    println!("Error {:?}", e);
                }
                None => break,
            }
        }
    }
    // client is disconnected, let's remove them from the state
    Ok(())
}
