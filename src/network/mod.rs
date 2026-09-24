pub mod client_side;
use crate::types::{Client, ClientInfo, ClientRequest, InboxEntry, Room, ServerEvent};
use client_side::run_client;

pub mod server_side;
use server_side::run_server;

pub mod framing;

use crate::WireError;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum NetworkCommand {
    SpawnClient { username: String },
    CreateRoom { username: String },
    SendMessage { data: InboxEntry },
    JoinRoom { client_id: u64, room_id: u64 },
}

#[derive(Debug)]
pub enum NetworkEvent {
    ClientConnected(ClientInfo),
    ClientDisconnected { id: u64 },
    RoomCreated { room: Room },
    JoinedRoom { client_id: u64, room_id: u64 },
    MessageReceived(InboxEntry),
    ErrorOccurred(WireError),
}

#[derive(Clone)]
pub struct NetworkHandle {
    pub cmd_tx: Sender<NetworkCommand>,
}

impl NetworkHandle {
    pub fn new(cmd_tx: Sender<NetworkCommand>) -> Self {
        NetworkHandle { cmd_tx }
    }

    pub fn spawn_client(&self, name: String) -> Result<(), WireError> {
        self.cmd_tx
            .try_send(NetworkCommand::SpawnClient { username: name })
            .map_err(|_| WireError::PortNotAvailable)
    }

    pub fn send_message(&self, data: InboxEntry) -> Result<(), WireError> {
        self.cmd_tx
            .try_send(NetworkCommand::SendMessage { data })
            .map_err(|_| WireError::ChannelFailure)
    }

    pub fn create_room(&self, name: String) -> Result<(), WireError> {
        self.cmd_tx
            .try_send(NetworkCommand::CreateRoom { username: name })
            .map_err(|_| WireError::ChannelFailure)
    }

    pub fn join_room(&self, client_id: u64, room_id: u64) -> Result<(), WireError> {
        self.cmd_tx
            .try_send(NetworkCommand::JoinRoom { client_id, room_id })
            .map_err(|_| WireError::ChannelFailure)
    }
}

pub struct ServerState {
    pub clients: Arc<Mutex<HashMap<u64, Client>>>,
    pub rooms: Arc<Mutex<HashMap<u64, Room>>>,
    pub next_id: Arc<atomic::AtomicU64>,
    pub next_room_id: Arc<atomic::AtomicU64>,
}

impl ServerState {
    pub async fn new() -> Self {
        ServerState {
            clients: Arc::new(Mutex::new(HashMap::new())),
            rooms: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(atomic::AtomicU64::new(0)),
            next_room_id: Arc::new(atomic::AtomicU64::new(0)),
        }
    }
    pub async fn register(&mut self, client: Client) {
        self.clients.lock().await.insert(client.id, client);
    }

    pub async fn remove(&mut self, id: u64) {
        self.clients.lock().await.remove(&id);
    }

    pub async fn get(&self, id: u64) -> Option<Client> {
        self.clients.lock().await.get(&id).cloned()
    }

    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, atomic::Ordering::SeqCst)
    }

    pub fn next_room_id(&self) -> u64 {
        self.next_room_id.fetch_add(1, atomic::Ordering::SeqCst)
    }
}

pub struct ClientState {
    pub client: Client,
    pub clients: Arc<Mutex<HashMap<u64, ClientInfo>>>,
    pub rooms: Arc<Mutex<HashMap<u64, Room>>>,
    pub event_tx: Sender<NetworkEvent>,
}

impl ClientState {
    pub async fn new(
        id: u64,
        username: String,
        cmd_tx: Option<Sender<ClientRequest>>,
        event_tx: Sender<NetworkEvent>,
        writer: Option<Sender<ServerEvent>>,
        ip: String,
        port: u16,
    ) -> Self {
        ClientState {
            client: Client::new(id, username, cmd_tx, writer, ip, port),
            clients: Arc::new(Mutex::new(HashMap::new())),
            rooms: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub async fn default(event_tx: Sender<NetworkEvent>) -> Self {
        Self::new(0, String::new(), None, event_tx, None, String::new(), 0).await
    }

    pub async fn register(&mut self, client: ClientInfo) {
        self.clients.lock().await.insert(client.id, client);
    }

    pub async fn remove(&mut self, id: u64) {
        self.clients.lock().await.remove(&id);
    }

    pub async fn get(&self, id: u64) -> Option<ClientInfo> {
        self.clients.lock().await.get(&id).cloned()
    }
}

pub async fn run_network_engine(
    cmd_rx: Receiver<NetworkCommand>,
    event_tx: Sender<NetworkEvent>,
) -> Result<(), WireError> {
    let server_addr = match run_server("127.0.0.1:0").await {
        Ok(addr) => addr,
        Err(e) => {
            panic!(
                "fatal: could not run server ({}). The network engine cannot start.",
                e
            );
        }
    };

    run_client(event_tx, cmd_rx, server_addr.as_str()).await?;

    Ok(())
}
