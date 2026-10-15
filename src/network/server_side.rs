use crate::db::init_server_db;
use crate::network::{ServerState, framing};
use crate::types::{Client, ClientInfo, ClientRequest, Room, ServerEvent};
use crate::{AlysonError, WireError};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

pub async fn accept_loop(listener: TcpListener, server_state: Arc<ServerState>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let server_state_inner = Arc::clone(&server_state);
                tokio::spawn(async move {
                    let _ = handle_incoming_connection(stream, server_state_inner).await;
                });
            }
            Err(_) => {}
        }
    }
}

// main server loop
pub async fn run_server(addr: String) -> Result<String, AlysonError> {
    let listener = match TcpListener::bind(addr).await {
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
    println!("Server started on {}", server_addr.clone());
    // initiate the server db
    let pool = init_server_db("sqlite://alyson.db").await?;

    // Map of active client connections on the server side
    let server_state = Arc::new(ServerState::new(pool).await);

    // Spawn the server accept loop
    let server_state_clone = Arc::clone(&server_state);
    // Server-side task that accepts incoming connections
    accept_loop(listener, server_state_clone).await;
    Ok(server_addr)
}

// Server Side Handling
pub async fn handle_incoming_connection(
    stream: TcpStream,
    server_state: Arc<ServerState>,
) -> Result<(), AlysonError> {
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
            return Err(AlysonError::Wire(WireError::InvalidName));
        }
    }
    if username.is_empty() {
        return Err(AlysonError::Wire(WireError::InvalidName));
    }

    let client_id = next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let peer_addr = writer.peer_addr()?;
    let ip = peer_addr.ip().to_string();
    let port = peer_addr.port();

    // Create server communication channel
    let (write_tx, write_rx) = tokio::sync::mpsc::channel::<ServerEvent>(32);

    // Create the new client
    let client = Client::new(
        client_id,
        username.clone(),
        None,
        Some(write_tx),
        ip.clone(),
        port,
    );

    let client_info = ClientInfo::new(client_id, username.clone(), ip.clone(), port);

    // Send the Client ID back to the client as part of the handshake protocol
    let msg = format!("{}\n", client_id);
    writer.write_all(msg.as_bytes()).await?;
    {
        let mut lock = clients.lock().await;
        lock.insert(client_id, client.clone());
    }

    // Notify all clients of the new connection
    for (_, client) in clients.lock().await.iter() {
        let Some(writer) = &client.writer else {
            continue;
        };
        let _ = writer
            .send(ServerEvent::PeerJoined(client_info.clone()))
            .await;
    }

    // Notify to the new client the list of all existing clients
    let mut roster: Vec<ClientInfo> = Vec::new();
    for (_, client) in clients.lock().await.iter() {
        roster.push(ClientInfo::new(
            client.id,
            client.username.clone(),
            client.ip.clone(),
            client.port,
        ));
    }
    if let Some(writer) = &client.writer {
        let _ = writer.send(ServerEvent::Roster(roster)).await;
    }

    // push the new client to the clients list
    clients.lock().await.insert(client_id, client.clone());

    // Server-side reader task of a Client
    let clients_reader = Arc::clone(&clients);
    let curr_client_r = client.clone();
    let rooms_reader = Arc::clone(&rooms);
    let server_state_clone = Arc::clone(&server_state);

    tokio::spawn(async move {
        let clients_reader_clone = clients_reader.clone();
        let rooms_reader_clone = rooms_reader.clone();
        let handle_entry = async move |msg: ClientRequest| match msg {
            ClientRequest::ChatMessage(entry) => {
                let clients_lock = clients_reader_clone.lock().await;
                let rooms_lock = rooms_reader_clone.lock().await;
                if entry.cli_or_room {
                    if let Some(client) = clients_lock.get(&entry.to) {
                        if let Some(writer) = &client.writer {
                            let _ = writer.send(ServerEvent::ChatMessage(entry.clone())).await;
                        }
                    }
                    if let Some(client) = clients_lock.get(&entry.from) {
                        if let Some(writer) = &client.writer {
                            let _ = writer.send(ServerEvent::ChatMessage(entry.clone())).await;
                        }
                    }
                } else {
                    if let Some(room) = rooms_lock.get(&entry.to) {
                        for cli_id in room.members.iter() {
                            if let Some(client) = clients_lock.get(cli_id) {
                                if let Some(writer) = &client.writer {
                                    let _ =
                                        writer.send(ServerEvent::ChatMessage(entry.clone())).await;
                                }
                            }
                        }
                    }
                }
            }
            ClientRequest::CreateRoom {
                client_id,
                username,
            } => {
                let id = server_state_clone.next_room_id();
                let new_room = Room::new(id, username, client_id);
                rooms_reader.lock().await.insert(id, new_room.clone());
                for (_, client) in clients_reader_clone.lock().await.iter() {
                    let Some(writer) = &client.writer else {
                        continue;
                    };
                    let _ = writer
                        .send(ServerEvent::RoomCreated(new_room.clone()))
                        .await;
                }
            }
            ClientRequest::JoinRoom { client_id, room_id } => {
                let rooms_reader_clone = rooms_reader.clone();
                if let Some(room) = rooms_reader_clone.lock().await.get_mut(&room_id) {
                    room.add_member(client_id);

                    let client = { clients_reader_clone.lock().await.get(&client_id).cloned() };
                    if let Some(client) = client {
                        if let Some(writer) = client.writer {
                            let _ = writer
                                .send(ServerEvent::RoomJoined {
                                    client_id,
                                    room: room.clone(),
                                })
                                .await;
                        }
                    }
                }
            }
        };
        let curr_client_er = curr_client_r.clone();
        let handle_error = async move |err: AlysonError| {
            let curr_client = curr_client_er.clone();
            let Some(writer) = curr_client.writer else {
                return;
            };
            let wire_err = match err {
                AlysonError::Wire(w) => w,
                _ => WireError::InternalServerError,
            };
            let _ = writer.send(ServerEvent::Error(wire_err)).await;
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
            .send(ServerEvent::ClientDisconnected(client_id))
            .await;
    });

    // Server-side writer task of a Client
    let curr_client_w = client.clone();
    tokio::spawn(async move {
        let handle_error = async move |err: AlysonError| {
            let curr_client = curr_client_w.clone();
            let Some(writer) = curr_client.writer else {
                return;
            };
            let wire_err = match err {
                AlysonError::Wire(w) => w,
                _ => WireError::InternalServerError,
            };
            let _ = writer.send(ServerEvent::Error(wire_err)).await;
        };
        framing::write_framed_loop(writer, write_rx, handle_error).await;
    });
    Ok(())
}
