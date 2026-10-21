use crate::AlysonError;
use crate::admin::metrics::ServerMetrics;
use crate::network::framing::read_framed_loop;
use crate::types::ServerEvent;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::watch;

/// A lightweight cooperative cancellation token using `tokio::sync::watch`.
///
/// Cloning this token creates another handle to the same cancellation broadcast.
/// Calling `.cancel()` notifies all waiting tasks simultaneously.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    tx: Arc<watch::Sender<bool>>,
    rx: watch::Receiver<bool>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    /// Create a new, uncancelled token.
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            tx: Arc::new(tx),
            rx,
        }
    }

    /// Request cancellation across all holders of this token.
    pub fn cancel(&self) {
        let _ = self.tx.send(true);
    }

    /// Check if cancellation has already been requested without waiting.
    pub fn is_cancelled(&self) -> bool {
        *self.rx.borrow()
    }

    /// Asynchronously wait until cancellation is signalled.
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        let mut rx = self.rx.clone();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }
}

/// Connects a headless client task to a managed server.
///
/// # Why Headless?
/// Headless clients run without any TUI or terminal rendering overhead.
/// They execute directly as lightweight `tokio` green threads (tasks), which consume
/// only a few hundred bytes each instead of a full OS process.
///
/// # Lifecycle:
/// 1. Establish TCP connection (`TcpStream::connect`).
/// 2. Perform protocol handshake (send username string, receive numeric client ID).
/// 3. Update `ServerMetrics` (bump `total_connects` and `active_connections`).
/// 4. Spawn a framed reader task to consume incoming server broadcasts.
/// 5. Wait until either:
///    - The `CancellationToken` is cancelled by the admin.
///    - The server closes the connection / drops TCP socket.
/// 6. On exit: decrement `active_connections`.
pub async fn headless_connect(
    server_addr: String,
    username: String,
    metrics: Arc<ServerMetrics>,
    cancel: CancellationToken,
) -> Result<u64, AlysonError> {
    // 1. Establish TCP connection
    let mut stream = match TcpStream::connect(&server_addr).await {
        Ok(s) => s,
        Err(e) => {
            metrics.record_connect_failure();
            return Err(AlysonError::from(e));
        }
    };

    // Disable Nagle's algorithm (TCP_NODELAY) for lower latency
    let _ = stream.set_nodelay(true);

    // 2. Protocol Handshake:
    // Write username terminated by newline
    let handshake = format!("{}\n", username);
    if let Err(e) = stream.write_all(handshake.as_bytes()).await {
        metrics.record_connect_failure();
        return Err(AlysonError::from(e));
    }

    // Split stream into reader and writer halves
    let (read_half, _write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Server responds with the assigned client ID on the first line
    let mut client_id_line = String::new();
    if let Err(e) = reader.read_line(&mut client_id_line).await {
        metrics.record_connect_failure();
        return Err(AlysonError::from(e));
    }

    let client_id: u64 = client_id_line
        .trim()
        .parse()
        .map_err(|e| {
            metrics.record_connect_failure();
            AlysonError::Internal(format!("Failed to parse server client_id: {}", e))
        })?;

    // 3. Mark successful connection in metrics
    metrics.record_connect();

    // 4. Spawn background framed reader loop
    let metrics_reader = Arc::clone(&metrics);
    let cancel_for_reader = cancel.clone();

    tokio::spawn(async move {
        // Callback invoked for every successfully deserialized ServerEvent
        let metrics_for_entry = Arc::clone(&metrics_reader);
        let on_entry = async move |msg: ServerEvent| -> Result<(), AlysonError> {
            metrics_for_entry
                .msgs_in
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let _ = msg;
            Ok(())
        };

        // Callback invoked on EOF or stream errors
        let on_error = async move |_err: AlysonError| {
            // Socket severed or EOF reached
        };

        // Select between reader loop and cancellation signal
        tokio::select! {
            _ = read_framed_loop(reader, on_entry, on_error) => {
                // Connection terminated by remote server
            }
            _ = cancel_for_reader.cancelled() => {
                // Client intentionally cancelled by admin
            }
        }

        // 5. Cleanup on disconnect
        metrics_reader.record_disconnect();
    });

    Ok(client_id)
}
