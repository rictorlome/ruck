use crate::conf::BROADCAST_CHANNEL_CAPACITY;
use crate::handshake::Handshake;
use anyhow::{anyhow, Result};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{copy, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

pub struct ServerConfig {
    pub max_clients: usize,
}

pub struct Shared {
    handshake_cache: HashMap<Bytes, OwnedWriteHalf>,
    config: ServerConfig,
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
    fn new(config: ServerConfig) -> Self {
        Shared {
            handshake_cache: HashMap::new(),
            config,
        }
    }

    fn is_at_capacity(&self) -> bool {
        self.handshake_cache.len() >= self.config.max_clients
    }
}

impl Client {
    async fn new(id: Bytes, state: State, socket: TcpStream) -> Result<Client> {
        let (read_socket, write_socket) = socket.into_split();
        let mut shared = state.lock().await;

        // Check if we're at capacity (but allow if peer is already waiting)
        let peer_socket = shared.handshake_cache.remove(&id);
        if peer_socket.is_none() && shared.is_at_capacity() {
            warn!(
                current = shared.handshake_cache.len(),
                max = shared.config.max_clients,
                "Server at capacity, rejecting connection"
            );
            return Err(anyhow!("Server at capacity"));
        }

        let client = Client {
            read_socket,
            peer_write_socket: peer_socket,
        };
        shared.handshake_cache.insert(id, write_socket);
        Ok(client)
    }

    async fn await_peer(
        state: State,
        id: &Bytes,
        id_channel: IdChannelReceiver,
        peer_timeout: Duration,
    ) -> Result<OwnedWriteHalf> {
        let mut id_channel = id_channel;

        let result = timeout(peer_timeout, async {
            loop {
                tokio::select! {
                    res = id_channel.recv() => match res {
                        Ok(bytes) if bytes == *id => {
                            match state.lock().await.handshake_cache.remove(id) {
                                Some(tx_write_half) => return tx_write_half,
                                _ => continue
                            }
                        },
                        _ => continue
                    },
                }
            }
        })
        .await;

        match result {
            Ok(socket) => Ok(socket),
            Err(_) => {
                // Clean up our entry from the cache on timeout
                state.lock().await.handshake_cache.remove(id);
                warn!(timeout_secs = peer_timeout.as_secs(), "Peer matching timed out");
                Err(anyhow!("Peer matching timed out"))
            }
        }
    }

    async fn upgrade(
        client: Client,
        state: State,
        handshake: Handshake,
        id_channel: IdChannelSender,
        peer_timeout: Duration,
    ) -> Result<StapledClient> {
        let mut peer_write_socket = match client.peer_write_socket {
            // Receiver - already stapled at creation
            Some(peer_write_socket) => peer_write_socket,
            // Sender - needs to wait for the incoming msg to look up peer_tx
            None => {
                Client::await_peer(state, &handshake.id, id_channel.subscribe(), peer_timeout)
                    .await?
            }
        };
        debug!("Peer connection established");
        peer_write_socket.write_all(&handshake.outbound_msg).await?;
        Ok(StapledClient {
            read_socket: client.read_socket,
            peer_write_socket,
        })
    }
}

pub async fn serve(bind: &str, max_clients: usize, timeout_secs: u64) -> Result<()> {
    let listener = TcpListener::bind(bind).await?;
    let config = ServerConfig { max_clients };
    info!(
        address = %bind,
        max_clients = max_clients,
        timeout_secs = timeout_secs,
        "Relay server listening"
    );
    let state = Arc::new(Mutex::new(Shared::new(config)));
    let (tx, _rx) = broadcast::channel::<Bytes>(BROADCAST_CHANNEL_CAPACITY);
    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let state = Arc::clone(&state);
        let tx = tx.clone();
        let peer_timeout = Duration::from_secs(timeout_secs);
        tokio::spawn(async move {
            match handle_connection(state, stream, tx, peer_timeout).await {
                Ok(_) => debug!(%peer_addr, "Connection complete"),
                Err(err) => error!(%peer_addr, error = %err, "Connection error"),
            }
        });
    }
}

pub async fn handle_connection(
    state: Arc<Mutex<Shared>>,
    socket: TcpStream,
    id_channel: IdChannelSender,
    peer_timeout: Duration,
) -> Result<()> {
    socket.readable().await?;
    let (handshake, socket) = Handshake::from_socket(socket).await?;
    let id = handshake.id.clone();
    let client = Client::new(id.clone(), state.clone(), socket).await?;
    id_channel.send(id.clone())?;
    debug!("Client registered, awaiting peer");
    let mut client =
        match Client::upgrade(client, state.clone(), handshake, id_channel, peer_timeout).await {
            Ok(client) => client,
            Err(err) => {
                // Clear handshake cache if staple is unsuccessful
                state.lock().await.handshake_cache.remove(&id);
                return Err(err);
            }
        };
    debug!("Clients paired, relaying traffic");
    // The handshake cache should be empty for {id} at this point.
    copy(&mut client.read_socket, &mut client.peer_write_socket).await?;
    Ok(())
}
