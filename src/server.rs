use crate::message::{HandshakePayload, Message, MessageStream, RuckError};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures::prelude::*;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

type Tx = mpsc::UnboundedSender<Message>;
type Rx = mpsc::UnboundedReceiver<Message>;

pub struct Shared {
    handshake_cache: HashMap<Bytes, Tx>,
}
type State = Arc<Mutex<Shared>>;

struct Client {
    messages: MessageStream,
    rx: Rx,
    peer_tx: Option<Tx>,
}
struct StapledClient {
    messages: MessageStream,
    rx: Rx,
    peer_tx: Tx,
}

impl Shared {
    fn new() -> Self {
        Shared {
            handshake_cache: HashMap::new(),
        }
    }
}

impl Client {
    async fn new(id: Bytes, state: State, messages: MessageStream) -> Result<Client> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut shared = state.lock().await;
        let client = Client {
            messages,
            rx,
            peer_tx: shared.handshake_cache.remove(&id),
        };
        shared.handshake_cache.insert(id, tx);
        Ok(client)
    }

    async fn upgrade(
        client: Client,
        state: State,
        handshake_payload: HandshakePayload,
    ) -> Result<StapledClient> {
        let mut client = client;
        let peer_tx = match client.peer_tx {
            // Receiver - already stapled at creation
            Some(peer_tx) => peer_tx,
            // Sender - needs to wait for the incoming msg to look up peer_tx
            None => {
                match client.rx.recv().await {
                    Some(msg) => client.messages.send(msg).await?,
                    None => return Err(anyhow!("Connection not stapled")),
                };
                match state
                    .lock()
                    .await
                    .handshake_cache
                    .remove(&handshake_payload.id)
                {
                    Some(peer_tx) => peer_tx,
                    None => return Err(anyhow!("Connection not stapled")),
                }
            }
        };
        peer_tx.send(Message::HandshakeMessage(handshake_payload))?;
        Ok(StapledClient {
            messages: client.messages,
            rx: client.rx,
            peer_tx,
        })
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
    // println!("server - received msg from {:?}", addr);
    let client = Client::new(handshake_payload.id.clone(), state.clone(), stream).await?;
    let mut client = Client::upgrade(client, state.clone(), handshake_payload).await?;
    loop {
        tokio::select! {
            Some(msg) = client.rx.recv() => {
                // println!("message received to client.rx {:?}", msg);
                client.messages.send(msg).await?
            }
            result = client.messages.next() => match result {
                Some(Ok(msg)) => {
                    client.peer_tx.send(msg)?
                }
                Some(Err(e)) => {
                    println!("Error {:?}", e);
                }
                None => break,
            }
        }
    }
    Ok(())
}
