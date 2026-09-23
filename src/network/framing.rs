use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::Receiver;

use crate::WireError;
use crate::types::WireMessage;
use std::sync::Arc;

pub async fn read_framed_loop<R: AsyncReadExt + Unpin>(
    mut reader: R,
    // event_tx_reader: Sender<NetworkEvent>,
    on_entry: impl AsyncFn(WireMessage),
    on_error: impl AsyncFn(WireError),
) {
    // safe payload cap(10MB)
    const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;
    loop {
        let mut len_bytes = [0u8; 4];

        // 1. Read message length header
        if let Err(e) = reader.read_exact(&mut len_bytes).await {
            // EOF or socket closed: clean exit
            on_error(WireError::Io(Arc::new(e))).await;
            break; // <--- BREAK: Socket is closed or unreachable
        }

        let len = u32::from_be_bytes(len_bytes) as usize;

        // 2. Validate payload length to prevent OOM panics
        if len > MAX_PAYLOAD_SIZE {
            on_error(WireError::PayloadTooLarge).await;
            break; // <--- BREAK: Stream is corrupt/untrusted
        }

        // 3. Read payload body
        let mut buffer = vec![0u8; len];
        if let Err(e) = reader.read_exact(&mut buffer).await {
            on_error(WireError::Io(Arc::new(e))).await;
            break; // <--- BREAK: Partial read failure / connection lost
        }

        // 4. Deserialize struct
        let msg: WireMessage = match bincode::deserialize(&buffer) {
            Ok(msg) => msg,
            Err(_) => {
                on_error(WireError::SerializationFailed).await;
                break; // <--- BREAK: Stream framing is out of alignment
            }
        };

        on_entry(msg).await;
    }
}

pub async fn write_framed_loop<W: AsyncWriteExt + Unpin>(
    mut writer: W,
    mut rx: Receiver<WireMessage>,
    on_error: impl AsyncFn(WireError),
) {
    while let Some(msg) = rx.recv().await {
        let bytes: Vec<u8> = match bincode::serialize(&msg) {
            Ok(bytes) => bytes,
            Err(_) => {
                on_error(WireError::SerializationFailed).await;
                continue;
            }
        };
        let len = (bytes.len() as u32).to_be_bytes();
        if writer.write_all(&len).await.is_err() {
            on_error(WireError::Disconnected).await;
            break;
        }
        if writer.write_all(&bytes).await.is_err() {
            on_error(WireError::Disconnected).await;
            break;
        }
    }
}
