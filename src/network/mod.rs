pub mod client_side;
use crate::db::init_server_db;
use crate::types::{Client, ClientInfo, ClientRequest, InboxEntry, Room};
use client_side::run_client;
use sqlx::SqlitePool;

pub mod framing;
pub mod server_side;

use crate::AlysonError;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum NetworkCommand {
    SpawnClient { username: String },
    CreateRoom { client_id: u64, username: String },
    SendMessage { data: InboxEntry },
    JoinRoom { client_id: u64, room_id: u64 },
}

#[derive(Debug)]
pub enum NetworkEvent {
    ClientConnected(ClientInfo),
    ClientDisconnected { id: u64 },
    RoomCreated { room: Room },
    RoomJoined { client_id: u64, room_id: u64 },
    MessageReceived(InboxEntry),
    MyClient { id: u64 },
    ErrorOccurred(AlysonError),
}

#[derive(Clone)]
pub struct NetworkHandle {
    pub cmd_tx: Sender<NetworkCommand>,
}

impl NetworkHandle {
    pub fn new(cmd_tx: Sender<NetworkCommand>) -> Self {
        NetworkHandle { cmd_tx }
    }

    pub fn spawn_client(&self, name: String) -> Result<(), AlysonError> {
        self.cmd_tx
            .try_send(NetworkCommand::SpawnClient { username: name })
            .map_err(|e| AlysonError::ChannelFailure(e.to_string()))
    }

    pub fn send_message(&self, data: InboxEntry) -> Result<(), AlysonError> {
        self.cmd_tx
            .try_send(NetworkCommand::SendMessage { data })
            .map_err(|e| AlysonError::ChannelFailure(e.to_string()))
    }

    pub fn create_room(&self, client_id: u64, name: String) -> Result<(), AlysonError> {
        self.cmd_tx
            .try_send(NetworkCommand::CreateRoom {
                client_id,
                username: name,
            })
            .map_err(|e| AlysonError::ChannelFailure(e.to_string()))
    }

    pub fn join_room(&self, client_id: u64, room_id: u64) -> Result<(), AlysonError> {
        self.cmd_tx
            .try_send(NetworkCommand::JoinRoom { client_id, room_id })
            .map_err(|e| AlysonError::ChannelFailure(e.to_string()))
    }
}

pub struct ServerState {
    pub db: SqlitePool,
    pub clients: Arc<Mutex<HashMap<u64, Client>>>,
    pub tmp_clients: Arc<Mutex<HashMap<u64, String>>>,
    pub rooms: Arc<Mutex<HashMap<u64, Room>>>,
}

impl ServerState {
    pub async fn new(db: SqlitePool) -> Self {
        ServerState {
            db,
            clients: Arc::new(Mutex::new(HashMap::new())),
            tmp_clients: Arc::new(Mutex::new(HashMap::new())),
            rooms: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn register_tmp_client(&self, id: u64, username: String) {
        self.tmp_clients.lock().await.insert(id, username);
    }

    pub async fn register(&self, client: Client) {
        self.clients.lock().await.insert(client.id, client);
    }

    pub async fn register_room(&self, room: Room) {
        self.rooms.lock().await.insert(room.id, room);
    }

    pub async fn remove(&self, id: u64) {
        self.clients.lock().await.remove(&id);
    }

    pub async fn get(&self, id: u64) -> Option<Client> {
        self.clients.lock().await.get(&id).cloned()
    }
}

pub struct ClientState {
    pub senders: Arc<Mutex<HashMap<u64, Sender<ClientRequest>>>>,
    pub clients: Arc<Mutex<HashMap<u64, ClientInfo>>>,
    pub rooms: Arc<Mutex<HashMap<u64, Room>>>,
    pub event_tx: Sender<NetworkEvent>,
}

impl ClientState {
    pub async fn new(event_tx: Sender<NetworkEvent>) -> Self {
        ClientState {
            senders: Arc::new(Mutex::new(HashMap::new())),
            clients: Arc::new(Mutex::new(HashMap::new())),
            rooms: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
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
) -> Result<(), AlysonError> {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(e) => {
            panic!(
                "fatal: could not bind TCP listener ({}). The network engine cannot start.",
                e
            );
        }
    };
    let server_addr = match listener.local_addr() {
        Ok(addr) => addr.to_string(),
        Err(e) => {
            panic!(
                "fatal: could not read local addr of TCP listener ({}). The network engine cannot start.",
                e
            );
        }
    };
    // initiate the server db
    let pool = init_server_db("sqlite://alyson.db").await?;

    // Map of active client connections on the server side
    let server_state = Arc::new(ServerState::new(pool).await);

    // Spawn the server accept loop
    let server_state_clone = Arc::clone(&server_state);
    // Server-side task that accepts incoming connections
    tokio::spawn(async move {
        server_side::accept_loop(listener, server_state_clone).await;
    });

    run_client(event_tx, cmd_rx, server_addr).await?;

    Ok(())
}
