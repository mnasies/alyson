use crate::WireError;
use crate::network::{NetworkEvent, ServerState, framing};
use crate::types::{Client, InboxEntry, WireMessage};
use tokio::io::AsyncWriteExt;

use std::sync::Arc;
use tokio::net::TcpStream;

// Server Side Handling
pub async fn handle_incoming_connection(
    stream: TcpStream,
    net_state: Arc<ServerState>,
) -> Result<(), WireError> {
    use tokio::io::AsyncBufReadExt;

    let clients = net_state.clients.clone();
    let rooms = net_state.rooms.clone();
    let next_id = net_state.next_id.clone();
    let event_tx = net_state.event_tx.clone();

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
        write_tx,
        ip.clone(),
        port,
    );
    // Send the Client ID to the client as part of the handshake protocol
    writer.write_all(&client_id.to_string().as_bytes()).await?;
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
    let rooms_reader = Arc::clone(&rooms);
    let event_tx_reader = event_tx.clone();
    tokio::spawn(async move {
        let clients_reader_clone = clients_reader.clone();
        let rooms_reader_clone = rooms_reader.clone();
        let handle_entry = async move |entry: InboxEntry| {
            // find the client and send the data to server-side writer task
            let clients_lock = clients_reader_clone.lock().await;
            let rooms_lock = rooms_reader_clone.lock().await;
            if entry.cli_or_room {
                if let Some(client) = clients_lock.get(&entry.to) {
                    let _ = client.writer.send(entry).await;
                }
            } else {
                if let Some(room) = rooms_lock.get(&entry.to) {
                    for cli_id in room.members.iter() {
                        if let Some(client) = clients_lock.get(cli_id) {
                            let _ = client.writer.send(entry.clone()).await;
                        }
                    }
                }
            }
        };
        framing::read_framed_loop(reader, event_tx_reader.clone(), handle_entry).await;

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
        framing::write_framed_loop(writer, write_rx, event_tx_writer).await;
    });
    Ok(())
}
