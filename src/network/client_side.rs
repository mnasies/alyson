use crate::WireError;
use crate::network::{ClientState, NetworkCommand, NetworkEvent, framing};
use crate::types::{Client, ClientRequest, InboxEntry, ServerEvent};

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};

pub async fn run_client(
    event_tx: Sender<NetworkEvent>,
    mut cmd_rx: Receiver<NetworkCommand>,
    server_port: u16,
) -> Result<(), WireError> {
    let client_state = Arc::new(Mutex::new(ClientState::default(event_tx).await));

    // Handle UI commands
    let client_state_lock = client_state.lock().await;
    let clients = Arc::clone(&client_state_lock.clients);
    let rooms = Arc::clone(&client_state_lock.rooms);
    let event_tx_clone = client_state_lock.event_tx.clone();
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            NetworkCommand::SpawnClient { username } => {
                let client_state_clone = Arc::clone(&client_state);
                tokio::spawn(async move {
                    if let Err(e) =
                        spawn_client_task(username, server_port, client_state_clone).await
                    {
                        let _ = event_tx_clone.send(NetworkEvent::ErrorOccurred(e)).await;
                    }
                });
            }
            NetworkCommand::SendMessage { data } => {
                let client = client_state_lock.client;
                match &client.cmd_tx {
                    Some(m) => {
                        let _ = m.send(ClientRequest::ChatMessage(data)).await;
                    }
                    None => {
                        let _ = event_tx_clone
                            .send(NetworkEvent::ErrorOccurred(WireError::ChannelNotFound))
                            .await;
                    }
                }
            }
            NetworkCommand::CreateRoom { username } => {
                let client = client_state_lock.client;
                match &client.cmd_tx {
                    Some(m) => {
                        let _ = m.send(ClientRequest::CreateRoom { username }).await;
                    }
                    None => {
                        let _ = event_tx_clone
                            .send(NetworkEvent::ErrorOccurred(WireError::ChannelNotFound))
                            .await;
                    }
                }
                // let room_id = client_state.next_room_id();
                // let room = Room::new(room_id, username.clone());
                // let _ = client_state
                //     .rooms
                //     .lock()
                //     .await
                //     .insert(room_id, room.clone());
                // let _ = event_tx_clone
                //     .send(NetworkEvent::RoomCreated { room })
                //     .await;
            }
            NetworkCommand::JoinRoom { client_id, room_id } => {
                let client = client_state_lock.client;
                match &client.cmd_tx {
                    Some(m) => {
                        let _ = m.send(ClientRequest::JoinRoom { client_id, room_id }).await;
                    }
                    None => {
                        let _ = event_tx_clone
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
    server_port: u16,
    client_state: Arc<Mutex<ClientState>>,
) -> Result<Client, WireError> {
    use AsyncWriteExt;

    let client_state = client_state.lock().await;
    let clients = Arc::clone(&client_state.clients);
    let event_tx = client_state.event_tx.clone();

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", server_port)).await?;

    // Handshake: write username
    let handshake_msg = format!("{}\n", username);
    stream.write_all(handshake_msg.as_bytes()).await?;

    // Resolve client ID by reading the first line of the response
    let (read_half, writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut client_id = String::new();
    reader.read_line(&mut client_id).await?;
    let client_id: u64 = client_id.trim().parse().unwrap();

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
                _ => {}
            }
        };
        let event_tx_error = event_tx_reader.clone();
        let handle_error = async move |e: WireError| match e {
            e => {
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
                let _ = event_tx_writer.send(NetworkEvent::ErrorOccurred(e)).await;
            }
        };
        framing::write_framed_loop(writer, cmd_rx, handle_error).await;
    });

    Ok(client)
}
