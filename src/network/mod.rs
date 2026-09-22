pub mod client_side;
use client_side::spawn_client_task;

pub mod server_side;
use server_side::handle_incoming_connection;

pub mod framing;

use crate::WireError;
use crate::client::{Client, InboxEntry, Room};

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum NetworkCommand {
    SpawnClient { username: String },
    CreateRoom { username: String },
    SendMessage { data: InboxEntry },
    JoinRoom { client_id: usize, room_id: usize },
}

#[derive(Debug)]
pub enum NetworkEvent {
    ClientConnected {
        id: usize,
        username: String,
        ip: String,
        port: u16,
    },
    ClientDisconnected {
        id: usize,
    },
    RoomCreated {
        room: Room,
    },
    JoinedRoom {
        client_id: usize,
        room_id: usize,
    },
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

    pub fn join_room(&self, client_id: usize, room_id: usize) -> Result<(), WireError> {
        self.cmd_tx
            .try_send(NetworkCommand::JoinRoom { client_id, room_id })
            .map_err(|_| WireError::ChannelFailure)
    }
}

pub struct NetworkState {
    pub clients: Arc<Mutex<HashMap<usize, Client>>>,
    pub rooms: Arc<Mutex<HashMap<usize, Room>>>,
    pub next_id: Arc<atomic::AtomicUsize>,
    pub next_room_id: Arc<atomic::AtomicUsize>,
    pub event_tx: Sender<NetworkEvent>,
}

impl NetworkState {
    pub async fn new(event_tx: Sender<NetworkEvent>) -> Self {
        NetworkState {
            clients: Arc::new(Mutex::new(HashMap::new())),
            rooms: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(atomic::AtomicUsize::new(0)),
            next_room_id: Arc::new(atomic::AtomicUsize::new(0)),
            event_tx,
        }
    }
    pub async fn register(&mut self, client: Client) {
        self.clients.lock().await.insert(client.id, client);
    }

    pub async fn remove(&mut self, id: usize) {
        self.clients.lock().await.remove(&id);
    }

    pub async fn get(&self, id: usize) -> Option<Client> {
        self.clients.lock().await.get(&id).cloned()
    }

    pub fn next_id(&self) -> usize {
        self.next_id.fetch_add(1, atomic::Ordering::SeqCst)
    }

    pub fn next_room_id(&self) -> usize {
        self.next_room_id.fetch_add(1, atomic::Ordering::SeqCst)
    }
}

pub async fn run_network_engine(
    mut cmd_rx: Receiver<NetworkCommand>,
    event_tx: Sender<NetworkEvent>,
) -> Result<(), WireError> {
    // 1. Bind TCP server to random port.
    // These two failures are unrecoverable: the network engine cannot function
    // without a listener, so we surface a fatal error and let the panic hook
    // reset the terminal and exit.
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(e) => {
            panic!(
                "fatal: could not bind TCP listener ({}). The network engine cannot start.",
                e
            );
        }
    };

    let server_port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => {
            panic!(
                "fatal: could not read local addr of TCP listener ({}). The network engine cannot start.",
                e
            );
        }
    };

    // Map of active client connections on the server side
    let net_state = Arc::new(NetworkState::new(event_tx.clone()).await);

    // Spawn the server accept loop
    let net_state_clone = Arc::clone(&net_state);
    // Server-side task that accepts incoming connections
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let event_tx = net_state_clone.event_tx.clone();
                    let net_state_inner = Arc::clone(&net_state_clone);
                    tokio::spawn(async move {
                        if let Err(e) = handle_incoming_connection(stream, net_state_inner).await {
                            let _ = event_tx.send(NetworkEvent::ErrorOccurred(e)).await;
                        }
                    });
                }
                Err(e) => {
                    let _ = event_tx
                        .send(NetworkEvent::ErrorOccurred(WireError::TcpConnectionFailed(
                            Arc::new(e),
                        )))
                        .await;
                }
            }
        }
    });

    // Handle UI commands
    let clients = Arc::clone(&net_state.clients);
    let rooms = Arc::clone(&net_state.rooms);
    let event_tx_clone = net_state.event_tx.clone();
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            NetworkCommand::SpawnClient { username } => {
                let event_tx_clone = net_state.event_tx.clone();
                let net_state_client = Arc::clone(&net_state);
                tokio::spawn(async move {
                    if let Err(e) = spawn_client_task(username, server_port, net_state_client).await
                    {
                        let _ = event_tx_clone.send(NetworkEvent::ErrorOccurred(e)).await;
                    }
                });
            }
            NetworkCommand::SendMessage { data } => {
                let clients_lock = clients.lock().await;
                if let Some(client) = clients_lock.get(&data.from) {
                    match &client.cmd_tx {
                        Some(m) => {
                            let _ = m.send(data).await;
                        }
                        None => {
                            let _ = event_tx_clone
                                .send(NetworkEvent::ErrorOccurred(WireError::ChannelNotFound))
                                .await;
                        }
                    }
                }
            }
            NetworkCommand::CreateRoom { username } => {
                let room_id = net_state.next_room_id();
                let room = Room::new(room_id, username.clone());
                let _ = net_state.rooms.lock().await.insert(room_id, room.clone());
                let _ = event_tx_clone
                    .send(NetworkEvent::RoomCreated { room })
                    .await;
            }
            NetworkCommand::JoinRoom { client_id, room_id } => {
                let mut rooms_lock = rooms.lock().await;
                for (id, room) in rooms_lock.iter_mut() {
                    if *id == room_id {
                        room.add_member(client_id);
                    }
                }
                let _ = event_tx_clone
                    .send(NetworkEvent::JoinedRoom { client_id, room_id })
                    .await;
            }
        }
    }
    Ok(())
}
