use crate::AlysonError;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::Receiver;

pub async fn read_framed_loop<R, T, E>(
    mut reader: R,
    on_entry: impl AsyncFn(T),
    on_error: impl AsyncFn(E),
) where
    R: AsyncReadExt + Unpin,
    T: for<'de> Deserialize<'de>,
    E: From<AlysonError>,
{
    // safe payload cap (10MB)
    const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024;
    loop {
        let mut len_bytes = [0u8; 4];

        // 1. Read message length header
        if let Err(e) = reader.read_exact(&mut len_bytes).await {
            // EOF or socket closed: clean exit
            on_error(E::from(AlysonError::from(e))).await;
            break;
        }

        let len = u32::from_be_bytes(len_bytes) as usize;

        // 2. Validate payload length to prevent OOM panics
        if len > MAX_PAYLOAD_SIZE {
            on_error(E::from(AlysonError::Wire(
                crate::WireError::PayloadTooLarge,
            )))
            .await;
            break;
        }

        // 3. Read payload body
        let mut buffer = vec![0u8; len];
        if let Err(e) = reader.read_exact(&mut buffer).await {
            on_error(E::from(AlysonError::from(e))).await;
            break;
        }

        // 4. Deserialize struct
        let msg: T = match bincode::deserialize(&buffer) {
            Ok(msg) => msg,
            Err(e) => {
                on_error(E::from(AlysonError::Serialization(e.to_string()))).await;
                break;
            }
        };

        on_entry(msg).await;
    }
}

pub async fn write_framed_loop<W, T, E>(
    mut writer: W,
    mut rx: Receiver<T>,
    on_error: impl AsyncFn(E),
) where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
    E: From<AlysonError>,
{
    while let Some(msg) = rx.recv().await {
        let bytes: Vec<u8> = match bincode::serialize(&msg) {
            Ok(bytes) => bytes,
            Err(e) => {
                on_error(E::from(AlysonError::Serialization(e.to_string()))).await;
                continue;
            }
        };
        let len = (bytes.len() as u32).to_be_bytes();
        if let Err(e) = writer.write_all(&len).await {
            on_error(E::from(AlysonError::from(e))).await;
            break;
        }
        if let Err(e) = writer.write_all(&bytes).await {
            on_error(E::from(AlysonError::from(e))).await;
            break;
        }
    }
}
