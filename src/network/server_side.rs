use crate::WireError;
use crate::network::{NetworkEvent, ServerState, client_side, framing};
use crate::types::{Client, ClientInfo, InboxEntry, WireMessage};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

// main server loop
pub async fn run_server(addr: &str) -> Result<u16, WireError> {
    let listener = match TcpListener::bind(addr).await {
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
    let server_state = Arc::new(ServerState::new(event_tx.clone()).await);

    // Spawn the server accept loop
    let server_state_clone = Arc::clone(&server_state);
    // Server-side task that accepts incoming connections
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let server_state_inner = Arc::clone(&server_state_clone);
                    tokio::spawn(async move {
                        let _ = handle_incoming_connection(stream, server_state_inner).await;
                    });
                }
                Err(e) => {}
            }
        }
    });
    Ok(server_port)
}

// Server Side Handling
pub async fn handle_incoming_connection(
    stream: TcpStream,
    server_state: Arc<ServerState>,
) -> Result<(), WireError> {
    use tokio::io::AsyncBufReadExt;

    let clients = server_state.clients.clone();
    let rooms = server_state.rooms.clone();
    let next_id = server_state.next_id.clone();

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
    let (write_tx, write_rx) = tokio::sync::mpsc::channel::<WireMessage>(32);

    let client = Client::new(
        client_id,
        username.clone(),
        None,
        Some(write_tx),
        ip.clone(),
        port,
    );

    let client_info = ClientInfo::new(client_id, username.clone(), ip.clone(), port);

    // Send the Client ID to the client as part of the handshake protocol
    writer.write_all(&client_id.to_string().as_bytes()).await?;
    {
        let mut lock = clients.lock().await;
        lock.insert(client_id, client.clone());
    }

    // Notify all clients of the new connection
    for (_, client) in clients.lock().await.iter() {
        let Some(writer) = client.writer else {
            continue;
        };
        let _ = writer
            .send(WireMessage::PeerJoined(client_info.clone()))
            .await;
    }

    // Server-side reader task of a Client
    let clients_reader = Arc::clone(&clients);
    let curr_client_r = client.clone();
    let rooms_reader = Arc::clone(&rooms);
    // let event_tx_reader = event_tx.clone();
    tokio::spawn(async move {
        let clients_reader_clone = clients_reader.clone();
        let rooms_reader_clone = rooms_reader.clone();
        let handle_entry = async move |msg: WireMessage| {
            match msg {
                WireMessage::ChatMessage(entry) => {
                    // find the client and send the data to server-side writer task
                    let clients_lock = clients_reader_clone.lock().await;
                    let rooms_lock = rooms_reader_clone.lock().await;
                    if entry.cli_or_room {
                        if let Some(client) = clients_lock.get(&entry.to) {
                            let _ = client.writer.send(WireMessage::ChatMessage(entry)).await;
                        }
                    } else {
                        if let Some(room) = rooms_lock.get(&entry.to) {
                            for cli_id in room.members.iter() {
                                if let Some(client) = clients_lock.get(cli_id) {
                                    let _ =
                                        client.writer.send(WireMessage::ChatMessage(entry)).await;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        };
        let curr_client_er = curr_client_r.clone();
        let handle_error = async move |err: WireError| match err {
            e => {
                let Some(writer) = curr_client_er.writer else {
                    return;
                };
                let _ = writer.send(WireMessage::Error(e.to_string())).await;
            }
        };
        framing::read_framed_loop(reader, handle_entry, handle_error).await;

        // Clean up client on disconnect
        {
            let mut lock = clients_reader.lock().await;
            lock.remove(&client_id);
        }
        let Some(writer) = curr_client_r.writer else {
            return;
        };
        let _ = writer
            .send(WireMessage::ClientDisconnected(client_id))
            .await;
    });

    // Server-side writer task of a Client

    let curr_client_w = client.clone();
    tokio::spawn(async move {
        let handle_error = async move |err: WireError| match err {
            e => {
                let Some(writer) = curr_client_w.writer else {
                    return;
                };
                let _ = writer.send(WireMessage::Error(e.to_string())).await;
            }
        };
        framing::write_framed_loop(writer, write_rx, handle_error).await;
    });
    Ok(())
}
