use crate::WireError;
use crate::network::{ClientState, NetworkCommand, NetworkEvent, framing};
use crate::types::{Client, ClientInfo, ClientRequest, ServerEvent};

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};

pub async fn run_client(
    event_tx: Sender<NetworkEvent>,
    mut cmd_rx: Receiver<NetworkCommand>,
    server_addr: String,
) -> Result<(), WireError> {
    let client_state = Arc::new(Mutex::new(ClientState::new(event_tx.clone()).await));

    // let (clients, rooms) = {
    //     let client_state_lock = client_state.lock().await;
    //     (
    //         Arc::clone(&client_state_lock.clients),
    //         Arc::clone(&client_state_lock.rooms),
    //     )
    // };

    // Handle UI commands
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            NetworkCommand::SpawnClient { username } => {
                let client_state_clone = Arc::clone(&client_state);
                let server_addr_clone = server_addr.clone();
                let event_tx_clone = event_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        spawn_client_task(username, server_addr_clone, client_state_clone).await
                    {
                        let _ = event_tx_clone.send(NetworkEvent::ErrorOccurred(e)).await;
                    }
                });
            }
            NetworkCommand::SendMessage { data } => {
                let sender = {
                    let client_state_lock = client_state.lock().await;
                    client_state_lock.senders.lock().await.get(&data.from)
                };
                match &sender {
                    Some(m) => {
                        let _ = m.send(ClientRequest::ChatMessage(data)).await;
                    }
                    None => {
                        let _ = event_tx
                            .send(NetworkEvent::ErrorOccurred(WireError::ChannelNotFound))
                            .await;
                    }
                }
            }
            NetworkCommand::CreateRoom {
                client_id,
                username,
            } => {
                let sender = {
                    let client_state_lock = client_state.lock().await;
                    client_state_lock.senders.lock().await.get(&client_id)
                };
                match &sender {
                    Some(m) => {
                        let _ = m.send(ClientRequest::CreateRoom { username }).await;
                    }
                    None => {
                        let _ = event_tx
                            .send(NetworkEvent::ErrorOccurred(WireError::ChannelNotFound))
                            .await;
                    }
                }
            }
            NetworkCommand::JoinRoom { client_id, room_id } => {
                let sender = {
                    let client_state_lock = client_state.lock().await;
                    client_state_lock.senders.lock().await.get(&client_id)
                };
                match &sender {
                    Some(m) => {
                        let _ = m.send(ClientRequest::JoinRoom { client_id, room_id }).await;
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
    Ok(())
}

// Client Side Handling
pub async fn spawn_client_task(
    username: String,
    server_addr: String,
    client_state: Arc<Mutex<ClientState>>,
) -> Result<Client, WireError> {
    use AsyncWriteExt;

    let (clients, event_tx) = {
        let client_state = client_state.lock().await;
        (
            Arc::clone(&client_state.clients),
            client_state.event_tx.clone(),
        )
    };

    let mut stream = TcpStream::connect(server_addr).await?;
    let ip = stream.peer_addr()?.ip().to_string();
    let port = stream.peer_addr()?.port();

    // Handshake: write username
    let handshake_msg = format!("{}\n", username);
    stream.write_all(handshake_msg.as_bytes()).await?;

    // Resolve client ID by reading the first line of the server response
    let (read_half, writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut client_id = String::new();
    reader.read_line(&mut client_id).await?;
    let client_id: u64 = client_id.trim().parse().unwrap();

    // Send the client ID back to the UI side
    event_tx
        .send(NetworkEvent::MyClient { id: client_id })
        .await
        .map_err(|_| WireError::ChannelNotFound)?;

    // Add the client to the clients list
    {
        let mut lock = clients.lock().await;
        lock.insert(
            client_id,
            ClientInfo::new(client_id, username.clone(), ip, port),
        );
    }

    // Create client writer channel
    let (cmd_tx, cmd_rx) = mpsc::channel::<ClientRequest>(32);

    // Assign client
    let client = {
        let mut lock = clients.lock().await;
        if let Some(client) = lock.get_mut(&client_id) {
            Client::new(
                client.id,
                client.username.clone(),
                Some(cmd_tx),
                None,
                client.ip.clone(),
                client.port,
            )
        } else {
            return Err(WireError::ClientNotFound);
        }
    };

    // Assign the ClientRequest sender
    {
        let client_state_lock = client_state.lock().await;
        if let Some(sender) = client.cmd_tx.clone() {
            client_state_lock
                .senders
                .lock()
                .await
                .insert(client_id, sender);
        }
    }

    let event_tx_reader = event_tx.clone();

    // Client-side reader task
    tokio::spawn(async move {
        let event_tx_entry = event_tx_reader.clone();
        let handle_entry = async move |msg: ServerEvent| {
            match msg {
                ServerEvent::ChatMessage(entry) => {
                    // 5. Forward to UI / Event handler
                    if event_tx_entry
                        .send(NetworkEvent::MessageReceived(entry))
                        .await
                        .is_err()
                    {
                        // Channel receiver was dropped (UI closed/shutdown)
                        return;
                    }
                }
                ServerEvent::PeerJoined(client_info) => {
                    if event_tx_entry
                        .send(NetworkEvent::ClientConnected(client_info))
                        .await
                        .is_err()
                    {
                        // Channel receiver was dropped (UI closed/shutdown)
                        return;
                    }
                }
                ServerEvent::Roster(clients) => {
                    for client in clients {
                        if event_tx_entry
                            .send(NetworkEvent::ClientConnected(client))
                            .await
                            .is_err()
                        {
                            // Channel receiver was dropped (UI closed/shutdown)
                            return;
                        }
                    }
                }
                ServerEvent::ClientDisconnected(id) => {
                    let _ = event_tx_entry
                        .send(NetworkEvent::ClientDisconnected { id })
                        .await;
                }
                // ServerEvent::Error(err) => {
                //     let _ = event_tx_entry.send(NetworkEvent::ErrorOccurred(err)).await;
                // }
                _ => {}
            }
        };
        let event_tx_error = event_tx_reader.clone();
        let handle_error = async move |e: WireError| match e {
            e => {
                eprintln!("client reader error: {:?}", e);
                let _ = event_tx_error.send(NetworkEvent::ErrorOccurred(e)).await;
            }
        };
        framing::read_framed_loop(reader, handle_entry, handle_error).await;

        // Surface the disconnect to the UI as a toast + sidebar cleanup.
        let _ = event_tx_reader
            .send(NetworkEvent::ClientDisconnected { id: client_id })
            .await;
    });

    let event_tx_writer = event_tx.clone();

    // Client-side writer task
    tokio::spawn(async move {
        let handle_error = async move |e: WireError| match e {
            e => {
                eprintln!("client reader error: {:?}", e);
                let _ = event_tx_writer.send(NetworkEvent::ErrorOccurred(e)).await;
            }
        };
        framing::write_framed_loop(writer, cmd_rx, handle_error).await;
    });

    Ok(client)
}
