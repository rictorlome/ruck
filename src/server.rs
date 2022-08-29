use anyhow::{anyhow, Result};
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

type Tx = mpsc::UnboundedSender<Bytes>;
type Rx = mpsc::UnboundedReceiver<Bytes>;

pub struct Shared {
    handshake_cache: HashMap<Bytes, Tx>,
}
type State = Arc<Mutex<Shared>>;

struct Client {
    socket: TcpStream,
    rx: Rx,
    peer_tx: Option<Tx>,
}
struct StapledClient {
    socket: TcpStream,
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
    async fn new(id: Bytes, state: State, socket: TcpStream) -> Result<Client> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut shared = state.lock().await;
        let client = Client {
            socket,
            rx,
            peer_tx: shared.handshake_cache.remove(&id),
        };
        shared.handshake_cache.insert(id, tx);
        Ok(client)
    }

    async fn upgrade(
        client: Client,
        state: State,
        id: Bytes,
        handshake_msg: Bytes,
    ) -> Result<StapledClient> {
        let mut client = client;
        let peer_tx = match client.peer_tx {
            // Receiver - already stapled at creation
            Some(peer_tx) => peer_tx,
            // Sender - needs to wait for the incoming msg to look up peer_tx
            None => {
                tokio::select! {
                    Some(msg) = client.rx.recv() => {
                        client.socket.write_all(&msg[..]).await?
                    }
                }
                match state.lock().await.handshake_cache.remove(&id) {
                    Some(peer_tx) => peer_tx,
                    None => return Err(anyhow!("Connection not stapled")),
                }
            }
        };
        peer_tx.send(handshake_msg)?;
        Ok(StapledClient {
            socket: client.socket,
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
    _addr: SocketAddr,
) -> Result<()> {
    socket.readable().await?;
    let mut socket = socket;
    // let mut handshake_buffer = BytesMut::with_capacity(55);
    let mut buffer = [0; 32 + 33];
    let n = socket.read_exact(&mut buffer).await?;
    println!("The bytes: {:?}", &buffer[..n]);
    let mut msg = BytesMut::from(&buffer[..n]).freeze();
    let id = msg.split_to(32);
    println!("New client with id={:?}, msg={:?}", id.clone(), msg.clone());
    let client = Client::new(id.clone(), state.clone(), socket).await?;
    println!("Client created");
    let mut client = match Client::upgrade(client, state.clone(), id.clone(), msg).await {
        Ok(client) => client,
        Err(err) => {
            // Clear handshake cache if staple is unsuccessful
            state.lock().await.handshake_cache.remove(&id);
            return Err(err);
        }
    };
    println!("Client upgraded");
    // The handshake cache should be empty for {id} at this point.
    let mut client_buffer = BytesMut::with_capacity(1024);
    loop {
        tokio::select! {
            Some(msg) = client.rx.recv() => {
                client.socket.write_all(&msg[..]).await?
            }
            result = client.socket.read(&mut client_buffer) => match result {
                Ok(0) => {
                    break
                },
                Ok(n) => {
                    println!("reading more");
                    client.peer_tx.send(BytesMut::from(&client_buffer[0..n]).freeze())?
                },
                Err(e) => {
                    println!("Error {:?}", e);
                }
            }
        }
    }
    println!("done with client");
    Ok(())
}
