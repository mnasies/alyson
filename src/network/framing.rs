use crate::client::WireMessage;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

use crate::WireError;
use crate::client::InboxEntry;
use crate::network::NetworkEvent;

pub async fn read_framed_loop<R: AsyncReadExt + Unpin>(
    mut reader: R,
    event_tx_reader: Sender<NetworkEvent>,
    on_entry: impl AsyncFn(InboxEntry),
) {
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

        on_entry(entry).await;
    }
}

pub async fn write_framed_loop<W: AsyncWriteExt + Unpin>(
    mut writer: W,
    mut rx: Receiver<WireMessage>,
    event_tx: Sender<NetworkEvent>,
) {
    while let Some(msg) = rx.recv().await {
        let bytes: Vec<u8> = match bincode::serialize(&msg) {
            Ok(bytes) => bytes,
            Err(_) => {
                let _ = event_tx
                    .send(NetworkEvent::ErrorOccurred(WireError::SerializationFailed))
                    .await;
                continue;
            }
        };
        let len = (bytes.len() as u32).to_be_bytes();
        if writer.write_all(&len).await.is_err() {
            let _ = event_tx
                .send(NetworkEvent::ErrorOccurred(WireError::Disconnected))
                .await;
            break;
        }
        if writer.write_all(&bytes).await.is_err() {
            let _ = event_tx
                .send(NetworkEvent::ErrorOccurred(WireError::Disconnected))
                .await;
            break;
        }
    }
}
