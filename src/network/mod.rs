use crate::WireError;
use crate::client::{Client, InboxEntry};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum NetworkCommand {
    SpawnClient { username: String },
    SendMessage { client_id: usize, msg: String },
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
            .blocking_send(NetworkCommand::SpawnClient { username: name })
            .map_err(|_| WireError::PortNotAvailable)
    }
}

pub async fn run_network_engine(
    mut cmd_rx: Receiver<NetworkCommand>,
    event_tx: Sender<NetworkEvent>,
) {
    // 1. Bind TCP server to random port
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(e) => {
            let _ = event_tx
                .send(NetworkEvent::ErrorOccurred(WireError::Io(e)))
                .await;
            return;
        }
    };

    let server_port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => {
            let _ = event_tx
                .send(NetworkEvent::ErrorOccurred(WireError::Io(e)))
                .await;
            return;
        }
    };

    // Map of active client connections on the server side
    let clients: Arc<tokio::sync::Mutex<HashMap<usize, Client>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let next_id = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Spawn the server accept loop
    let clients_clone = Arc::clone(&clients);
    let next_id_clone = Arc::clone(&next_id);
    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let clients_inner = Arc::clone(&clients_clone);
                    let next_id_inner = Arc::clone(&next_id_clone);
                    let event_tx_inner = event_tx_clone.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_incoming_connection(
                            stream,
                            clients_inner,
                            next_id_inner,
                            event_tx_inner,
                        )
                        .await
                        {
                            let _ = event_tx_inner.send(NetworkEvent::ErrorOccurred(e)).await;
                        }
                    });
                }
                Err(e) => {
                    let _ = event_tx_clone
                        .send(NetworkEvent::ErrorOccurred(WireError::TcpConnectionFailed(
                            e,
                        )))
                        .await;
                }
            }
        }
    });

    // Handle UI commands
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            NetworkCommand::SpawnClient { username } => {
                let event_tx_spawn = event_tx.clone();
                let clients_spawn = Arc::clone(&clients);
                tokio::spawn(async move {
                    if let Err(e) =
                        spawn_client_task(username, server_port, clients_spawn, event_tx_spawn)
                            .await
                    {
                        let _ = event_tx_spawn.send(NetworkEvent::ErrorOccurred(e)).await;
                    }
                });
            }
            NetworkCommand::SendMessage { client_id, msg } => {
                let clients_lock = clients.lock().await;
                if let Some(client) = clients_lock.get(&client_id) {
                    let _ = client.writer.send(msg).await;
                }
            }
        }
    }
}

async fn handle_incoming_connection(
    mut stream: TcpStream,
    clients: Arc<tokio::sync::Mutex<HashMap<usize, Client>>>,
    next_id: Arc<std::sync::atomic::AtomicUsize>,
    event_tx: Sender<NetworkEvent>,
) -> Result<(), WireError> {
    use tokio::io::AsyncBufReadExt;

    // Read first line as username (handshake)
    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut username = String::new();
    match reader.read_line(&mut username).await {
        Ok(_) => {
            username = username.trim().to_string();
        }
        Err(_) => {
            return Err(WireError::InvalidName);
        }
    }
    if username.is_empty() {
        return Err(WireError::InvalidName);
    }

    let client_id = next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let peer_addr = writer.peer_addr()?;
    let ip = peer_addr.ip().to_string();
    let port = peer_addr.port();

    // Create client writer channel
    let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<String>(32);

    // Spawn the writer task for this client connection
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        while let Some(msg) = write_rx.recv().await {
            let msg_newline = format!("{}\n", msg);
            if let Err(_) = writer.write_all(msg_newline.as_bytes()).await {
                break;
            }
        }
    });

    let client = Client::new(client_id, username.clone(), write_tx, ip.clone(), port);
    {
        let mut lock = clients.lock().await;
        lock.insert(client_id, client.clone());
    }

    // Notify UI of new connection
    let _ = event_tx
        .send(NetworkEvent::ClientConnected {
            id: client_id,
            username,
            ip,
            port,
        })
        .await;

    // Spawn the reader task to read incoming data from this client connection (server perspective)
    let clients_reader = Arc::clone(&clients);
    let event_tx_reader = event_tx.clone();
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => {
                    break;
                }
                Ok(_) => {
                    let msg = line.trim_end().to_string();
                    println!("{}", msg);

                    // Broadcast to other clients
                    let msg_bytes = format!("{}\n", msg);
                    let lock = clients_reader.lock().await;
                    for (&temp_id, temp_client) in lock.iter() {
                        if temp_id == client_id {
                            continue;
                        }
                        let _ = temp_client.writer.send(msg_bytes.clone()).await;
                    }
                }
            }
        }

        // Clean up client on disconnect
        {
            let mut lock = clients_reader.lock().await;
            lock.remove(&client_id);
        }
        let _ = event_tx_reader
            .send(NetworkEvent::ClientDisconnected { id: client_id })
            .await;
    });

    Ok(())
}

async fn spawn_client_task(
    username: String,
    server_port: u16,
    clients: Arc<tokio::sync::Mutex<HashMap<usize, Client>>>,
    event_tx: Sender<NetworkEvent>,
) -> Result<(), WireError> {
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", server_port)).await?;
    let local_port = stream.local_addr()?.port();

    // Handshake: write username
    let handshake_msg = format!("{}\n", username);
    stream.write_all(handshake_msg.as_bytes()).await?;

    // Resolve client ID using local port
    let client_id = resolve_client_id_by_port(&clients, local_port).await?;

    let (reader, _writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);

    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let entry = InboxEntry {
                        time: Instant::now(),
                        msg: line.trim().to_string(),
                        from: client_id,
                        to: client_id,
                        cli_or_room: true,
                    };
                    let _ = event_tx.send(NetworkEvent::MessageReceived(entry)).await;
                }
            }
        }
    });

    Ok(())
}

async fn resolve_client_id_by_port(
    clients: &Arc<tokio::sync::Mutex<HashMap<usize, Client>>>,
    local_port: u16,
) -> Result<usize, WireError> {
    let timeout = std::time::Duration::from_millis(500);
    let start = std::time::Instant::now();
    loop {
        {
            let lock = clients.lock().await;
            if let Some(client) = lock.values().find(|c| c.port == local_port) {
                return Ok(client.id);
            }
        }
        if start.elapsed() > timeout {
            return Err(WireError::ClientRegistrationTimeout);
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}
