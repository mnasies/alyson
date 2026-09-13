use crate::WireError;
use crate::client::{Client, InboxEntry};
use crate::network::{NetworkEvent, NetworkState, framing};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

// Client Side Handling
pub async fn spawn_client_task(
    username: String,
    server_port: u16,
    net_state: Arc<NetworkState>,
) -> Result<(), WireError> {
    use AsyncWriteExt;

    let clients = Arc::clone(&net_state.clients);
    let event_tx = net_state.event_tx.clone();

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", server_port)).await?;
    let local_port = stream.local_addr()?.port();

    // Handshake: write username
    let handshake_msg = format!("{}\n", username);
    stream.write_all(handshake_msg.as_bytes()).await?;

    // Resolve client ID using local port
    let client_id = resolve_client_id_by_port(&clients, local_port).await?;

    let (reader, writer) = stream.into_split();
    let reader = BufReader::new(reader);

    // Create client writer channel
    let (cmd_tx, cmd_rx) = mpsc::channel::<InboxEntry>(32);

    // Assign cmd_tx
    {
        let mut lock = clients.lock().await;
        if let Some(client) = lock.get_mut(&client_id) {
            client.cmd_tx = Some(cmd_tx);
        }
    }

    let event_tx_reader = event_tx.clone();

    // Client-side reader task
    tokio::spawn(async move {
        let event_tx_clone = event_tx_reader.clone();
        let handle_entry = async move |entry: InboxEntry| {
            // 5. Forward to UI / Event handler
            if event_tx_clone
                .send(NetworkEvent::MessageReceived(entry))
                .await
                .is_err()
            {
                // Channel receiver was dropped (UI closed/shutdown)
                return;
            }
        };
        framing::read_framed_loop(reader, event_tx_reader.clone(), handle_entry).await;

        // Surface the disconnect to the UI as a toast + sidebar cleanup.
        let _ = event_tx_reader
            .send(NetworkEvent::ClientDisconnected { id: client_id })
            .await;
    });

    let event_tx_writer = event_tx.clone();

    // Client-side writer task
    tokio::spawn(async move {
        framing::write_framed_loop(writer, cmd_rx, event_tx_writer).await;
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
