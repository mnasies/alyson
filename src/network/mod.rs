use crate::WireError;
use crate::client::{Client, InboxEntry};

use bincode;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum NetworkCommand {
    SpawnClient { username: String },
    SendMessage { data: InboxEntry },
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
            .try_send(NetworkCommand::SpawnClient { username: name })
            .map_err(|_| WireError::PortNotAvailable)
    }

    pub fn send_message(&self, data: InboxEntry) -> Result<(), WireError> {
        self.cmd_tx
            .try_send(NetworkCommand::SendMessage { data })
            .map_err(|_| WireError::PortNotAvailable)
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
    let clients: Arc<tokio::sync::Mutex<HashMap<usize, Client>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let next_id = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Spawn the server accept loop
    let clients_clone = Arc::clone(&clients);
    let next_id_clone = Arc::clone(&next_id);
    let event_tx_clone = event_tx.clone();
    // Server-side task that accepts incoming connections
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
                            event_tx_inner.clone(),
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
                            Arc::new(e),
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
                    if let Err(e) = spawn_client_task(
                        username,
                        server_port,
                        clients_spawn,
                        event_tx_spawn.clone(),
                    )
                    .await
                    {
                        let _ = event_tx_spawn.send(NetworkEvent::ErrorOccurred(e)).await;
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
                            let _ = event_tx
                                .send(NetworkEvent::ErrorOccurred(WireError::ChannelNotFound))
                                .await;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_incoming_connection(
    stream: TcpStream,
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

    // Create server communication channel
    let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<InboxEntry>(32);

    let client = Client::new(
        client_id,
        username.clone(),
        None,
        write_tx,
        ip.clone(),
        port,
    );
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

    // Server-side reader task of a Client
    let clients_reader = Arc::clone(&clients);
    let event_tx_reader = event_tx.clone();
    tokio::spawn(async move {
        // safe payload cap(10MB)
        const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;
        loop {
            let mut len_bytes = [0u8; 4];

            // 1. Read message length header
            if let Err(e) = reader.read_exact(&mut len_bytes).await {
                // EOF or socket closed: clean exit
                let _ = event_tx_reader
                    .send(NetworkEvent::ErrorOccurred(WireError::Io(Arc::new(e))))
                    .await;
                break; // <--- BREAK: Socket is closed or unreachable
            }

            let len = u32::from_be_bytes(len_bytes) as usize;

            // 2. Validate payload length to prevent OOM panics
            if len > MAX_PAYLOAD_SIZE {
                let _ = event_tx_reader
                    .send(NetworkEvent::ErrorOccurred(WireError::PayloadTooLarge))
                    .await;
                break; // <--- BREAK: Stream is corrupt/untrusted
            }

            // 3. Read payload body
            let mut buffer = vec![0u8; len];
            if let Err(e) = reader.read_exact(&mut buffer).await {
                let _ = event_tx_reader
                    .send(NetworkEvent::ErrorOccurred(WireError::Io(Arc::new(e))))
                    .await;
                break; // <--- BREAK: Partial read failure / connection lost
            }

            // 4. Deserialize struct
            let entry: InboxEntry = match bincode::deserialize(&buffer) {
                Ok(entry) => entry,
                Err(_) => {
                    let _ = event_tx_reader
                        .send(NetworkEvent::ErrorOccurred(WireError::SerializationFailed))
                        .await;
                    break; // <--- BREAK: Stream framing is out of alignment
                }
            };

            // find the client and the data to server-side writer task
            let clients_lock = clients.lock().await;
            if let Some(client) = clients_lock.get(&entry.to) {
                let _ = client.writer.send(entry).await;
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

    // Server-side writer task of a Client

    let event_tx_writer = event_tx.clone();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        while let Some(msg) = write_rx.recv().await {
            let bytes: Vec<u8> = match bincode::serialize(&msg) {
                Ok(bytes) => bytes,
                Err(_) => {
                    let _ = event_tx_writer
                        .send(NetworkEvent::ErrorOccurred(WireError::SerializationFailed))
                        .await;
                    continue;
                }
            };
            let len = (bytes.len() as u32).to_be_bytes();
            if writer.write_all(&len).await.is_err() {
                let _ = event_tx_writer
                    .send(NetworkEvent::ErrorOccurred(WireError::Disconnected))
                    .await;
                break;
            }
            if writer.write_all(&bytes).await.is_err() {
                let _ = event_tx_writer
                    .send(NetworkEvent::ErrorOccurred(WireError::Disconnected))
                    .await;
                break;
            }
        }
    });
    Ok(())
}

async fn spawn_client_task(
    username: String,
    server_port: u16,
    clients: Arc<tokio::sync::Mutex<HashMap<usize, Client>>>,
    event_tx: Sender<NetworkEvent>,
) -> Result<(), WireError> {
    use tokio::io::AsyncWriteExt;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", server_port)).await?;
    let local_port = stream.local_addr()?.port();

    // Handshake: write username
    let handshake_msg = format!("{}\n", username);
    stream.write_all(handshake_msg.as_bytes()).await?;

    // Resolve client ID using local port
    let client_id = resolve_client_id_by_port(&clients, local_port).await?;

    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);

    // Create client writer channel
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<InboxEntry>(32);

    // Mark this client as a read-side (client-initiated) connection
    {
        let mut lock = clients.lock().await;
        if let Some(client) = lock.get_mut(&client_id) {
            client.cmd_tx = Some(cmd_tx);
        }
    }

    let event_tx_reader = event_tx.clone();

    // Client-side reader task
    tokio::spawn(async move {
        // safe payload cap (10 MB)
        const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;

        loop {
            let mut len_bytes = [0u8; 4];

            // 1. Read message length header
            if let Err(e) = reader.read_exact(&mut len_bytes).await {
                // EOF or socket closed: clean exit
                let _ = event_tx_reader
                    .send(NetworkEvent::ErrorOccurred(WireError::Io(Arc::new(e))))
                    .await;
                break; // <--- BREAK: Socket is closed or unreachable
            }

            let len = u32::from_be_bytes(len_bytes) as usize;

            // 2. Validate payload length to prevent OOM panics
            if len > MAX_PAYLOAD_SIZE {
                let _ = event_tx_reader
                    .send(NetworkEvent::ErrorOccurred(WireError::PayloadTooLarge))
                    .await;
                break; // <--- BREAK: Stream is corrupt/untrusted
            }

            // 3. Read payload body
            let mut buffer = vec![0u8; len];
            if let Err(e) = reader.read_exact(&mut buffer).await {
                let _ = event_tx_reader
                    .send(NetworkEvent::ErrorOccurred(WireError::Io(Arc::new(e))))
                    .await;
                break; // <--- BREAK: Partial read failure / connection lost
            }

            // 4. Deserialize struct
            let entry: InboxEntry = match bincode::deserialize(&buffer) {
                Ok(entry) => entry,
                Err(_) => {
                    let _ = event_tx_reader
                        .send(NetworkEvent::ErrorOccurred(WireError::SerializationFailed))
                        .await;
                    break; // <--- BREAK: Stream framing is out of alignment
                }
            };

            // 5. Forward to UI / Event handler
            if event_tx_reader
                .send(NetworkEvent::MessageReceived(entry))
                .await
                .is_err()
            {
                // Channel receiver was dropped (UI closed/shutdown)
                break;
            }
        }
        // Surface the disconnect to the UI as a toast + sidebar cleanup.
        let _ = event_tx_reader
            .send(NetworkEvent::ClientDisconnected { id: client_id })
            .await;
    });

    let event_tx_writer = event_tx.clone();

    // Client-side writer task
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        while let Some(msg) = cmd_rx.recv().await {
            let bytes: Vec<u8> = match bincode::serialize(&msg) {
                Ok(bytes) => bytes,
                Err(_) => {
                    let _ = event_tx_writer
                        .send(NetworkEvent::ErrorOccurred(WireError::SerializationFailed))
                        .await;
                    continue;
                }
            };
            let len = (bytes.len() as u32).to_be_bytes();
            if writer.write_all(&len).await.is_err() {
                let _ = event_tx_writer
                    .send(NetworkEvent::ErrorOccurred(WireError::Disconnected))
                    .await;
                break;
            }
            if writer.write_all(&bytes).await.is_err() {
                let _ = event_tx_writer
                    .send(NetworkEvent::ErrorOccurred(WireError::Disconnected))
                    .await;
                break;
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
