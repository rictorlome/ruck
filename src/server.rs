use crate::handshake::Handshake;
use anyhow::Result;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{copy, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio::time::{sleep, Duration};

pub struct Shared {
    handshake_cache: HashMap<Bytes, OwnedWriteHalf>,
}
type State = Arc<Mutex<Shared>>;
type IdChannelSender = broadcast::Sender<bytes::Bytes>;
type IdChannelReceiver = broadcast::Receiver<bytes::Bytes>;

struct Client {
    read_socket: OwnedReadHalf,
    peer_write_socket: Option<OwnedWriteHalf>,
}
struct StapledClient {
    read_socket: OwnedReadHalf,
    peer_write_socket: OwnedWriteHalf,
}

impl Shared {
    fn new() -> Self {
        Shared {
            handshake_cache: HashMap::new(),
        }
    }
}

impl Client {
    async fn new(id: Bytes, state: State, socket: TcpStream) -> Result<Client> {
        let (read_socket, write_socket) = socket.into_split();
        let mut shared = state.lock().await;
        let client = Client {
            read_socket,
            peer_write_socket: shared.handshake_cache.remove(&id),
        };
        shared.handshake_cache.insert(id, write_socket);
        Ok(client)
    }

    async fn await_peer(state: State, id: &Bytes, id_channel: IdChannelReceiver) -> OwnedWriteHalf {
        let mut id_channel = id_channel;
        loop {
            tokio::select! {
                res = id_channel.recv() => match res {
                    Ok(bytes) if bytes == id => {
                        match state.lock().await.handshake_cache.remove(id) {
                            Some(tx_write_half) => {return tx_write_half},
                            _ => continue
                        }
                    },
                    _ => continue
                },
                else => {
                    sleep(Duration::from_millis(500)).await;
                    continue
                }
            }
        }
    }

    async fn upgrade(
        client: Client,
        state: State,
        handshake: Handshake,
        id_channel: IdChannelSender,
    ) -> Result<StapledClient> {
        let mut peer_write_socket = match client.peer_write_socket {
            // Receiver - already stapled at creation
            Some(peer_write_socket) => peer_write_socket,
            // Sender - needs to wait for the incoming msg to look up peer_tx
            None => Client::await_peer(state, &handshake.id, id_channel.subscribe()).await,
        };
        println!("past await peer");
        peer_write_socket.write_all(&handshake.outbound_msg).await?;
        Ok(StapledClient {
            read_socket: client.read_socket,
            peer_write_socket,
        })
    }
}

pub async fn serve() -> Result<()> {
    let addr = "127.0.0.1:8080".to_string();
    let listener = TcpListener::bind(&addr).await?;
    let state = Arc::new(Mutex::new(Shared::new()));
    let (tx, _rx) = broadcast::channel::<Bytes>(100);
    println!("Listening on: {}", addr);
    loop {
        let (stream, _address) = listener.accept().await?;
        let state = Arc::clone(&state);
        let tx = tx.clone();
        tokio::spawn(async move {
            match handle_connection(state, stream, tx).await {
                Ok(_) => println!("Connection complete!"),
                Err(err) => println!("Error handling connection! {:?}", err),
            }
        });
    }
}

pub async fn handle_connection(
    state: Arc<Mutex<Shared>>,
    socket: TcpStream,
    id_channel: IdChannelSender,
) -> Result<()> {
    socket.readable().await?;
    let (handshake, socket) = Handshake::from_socket(socket).await?;
    let id = handshake.id.clone();
    let client = Client::new(id.clone(), state.clone(), socket).await?;
    id_channel.send(id.clone())?;
    println!("Client created");
    let mut client = match Client::upgrade(client, state.clone(), handshake, id_channel).await {
        Ok(client) => client,
        Err(err) => {
            // Clear handshake cache if staple is unsuccessful
            state.lock().await.handshake_cache.remove(&id);
            return Err(err);
        }
    };
    println!("Client upgraded");
    // The handshake cache should be empty for {id} at this point.
    copy(&mut client.read_socket, &mut client.peer_write_socket).await?;
    Ok(())
}
