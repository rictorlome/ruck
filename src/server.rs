use crate::crypto::first_msg;
use crate::message::{Message, MessageStream};

use anyhow::{anyhow, Result};
use futures::prelude::*;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

type Tx = mpsc::UnboundedSender<Message>;
type Rx = mpsc::UnboundedReceiver<Message>;

pub struct Shared {
    rooms: HashMap<String, RoomInfo>,
}
type State = Arc<Mutex<Shared>>;

struct RoomInfo {
    sender_tx: Tx,
}

struct Client<'a> {
    is_sender: bool,
    messages: &'a mut MessageStream,
    rx: Rx,
}

impl Shared {
    fn new() -> Self {
        Shared {
            rooms: HashMap::new(),
        }
    }

    // async fn broadcast(&mut self, sender: SocketAddr, message: Message) {
    //     for peer in self.peers.iter_mut() {
    //         if *peer.0 != sender {
    //             let _ = peer.1.send(message.clone());
    //         }
    //     }
    // }
}

impl<'a> Client<'a> {
    async fn new(
        is_sender: bool,
        state: State,
        messages: &'a mut MessageStream,
    ) -> io::Result<Client<'a>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let room_info = RoomInfo { sender_tx: tx };
        state
            .lock()
            .await
            .rooms
            .insert("abc".to_string(), room_info);

        Ok(Client {
            is_sender,
            messages,
            rx,
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
                Ok(_) => println!("ok"),
                Err(_) => println!("err"),
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
    let (stream, first_msg) = first_msg(&mut stream).await?;
    let mut client = Client::new(true, state.clone(), stream).await?;
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
