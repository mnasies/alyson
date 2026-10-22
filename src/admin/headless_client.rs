use crate::AlysonError;
use crate::admin::metrics::ServerMetrics;
use crate::network::framing::{read_framed_loop, write_framed_loop};
use crate::types::{ClientRequest, InboxEntry, ServerEvent};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::watch;

/// Load testing scenario to run for connected headless clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Connect and hold connection open (tests socket capacity & listener queue).
    ConnectStorm,
    /// Connect and periodically send chat messages to measure round-trip latency and msg throughput.
    Chatty,
}

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
/// # Lifecycle:
/// 1. Establish TCP connection (`TcpStream::connect`), capturing fd limit errors (EMFILE/ENFILE) gracefully.
/// 2. Perform protocol handshake (send username string, receive numeric client ID).
/// 3. Update `ServerMetrics` (record connection latency, bump `total_connects` and `active_connections`).
/// 4. If `Scenario::Chatty`, spawn periodic message sender and framed writer.
/// 5. Spawn framed reader task to consume server events and measure round-trip latency.
/// 6. Wait until cancelled or severed, then cleanly decrement `active_connections`.
pub async fn headless_connect(
    server_addr: String,
    username: String,
    metrics: Arc<ServerMetrics>,
    cancel: CancellationToken,
    scenario: Scenario,
) -> Result<u64, AlysonError> {
    let start_time = Instant::now();

    // 1. Establish TCP connection
    let mut stream = match TcpStream::connect(&server_addr).await {
        Ok(s) => s,
        Err(e) => {
            // Check for OS file descriptor limit exhaustion (EMFILE / ENFILE)
            if let Some(code) = e.raw_os_error() {
                if code == libc::EMFILE || code == libc::ENFILE {
                    metrics.record_fd_limit_error();
                    return Err(AlysonError::from(e));
                }
            }
            metrics.record_connect_failure();
            return Err(AlysonError::from(e));
        }
    };

    // Disable Nagle's algorithm (TCP_NODELAY) for minimal latency
    let _ = stream.set_nodelay(true);

    // 2. Protocol Handshake:
    // Write username terminated by newline
    let handshake = format!("{}\n", username);
    if let Err(e) = stream.write_all(handshake.as_bytes()).await {
        metrics.record_connect_failure();
        return Err(AlysonError::from(e));
    }

    // Split stream into reader and writer halves
    let (read_half, write_half) = stream.into_split();
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

    // Record elapsed connection + handshake latency into histogram
    let elapsed = start_time.elapsed();
    metrics.record_connect_latency(elapsed);

    // 3. Mark successful connection in metrics
    metrics.record_connect();

    // 4. In Chatty scenario: spawn framed writer and periodic sender loop
    if scenario == Scenario::Chatty {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<ClientRequest>(32);
        let cancel_writer = cancel.clone();

        // Writer task
        tokio::spawn(async move {
            let on_write_err = async move |_err: AlysonError| {};
            tokio::select! {
                _ = write_framed_loop(write_half, cmd_rx, on_write_err) => {}
                _ = cancel_writer.cancelled() => {}
            }
        });

        // Periodic sender task (sends message every 150ms)
        let cancel_sender = cancel.clone();
        let metrics_sender = Arc::clone(&metrics);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(150));

            while !cancel_sender.is_cancelled() {
                tokio::select! {
                    _ = interval.tick() => {
                        let entry = InboxEntry::new(
                            SystemTime::now(),
                            "chatty probe".to_string(),
                            client_id,
                            client_id,
                            true,
                        );
                        if cmd_tx.send(ClientRequest::ChatMessage(entry)).await.is_ok() {
                            metrics_sender.msgs_out.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        } else {
                            break;
                        }
                    }
                    _ = cancel_sender.cancelled() => {
                        break;
                    }
                }
            }
        });
    }

    // 5. Spawn background framed reader loop
    let metrics_reader = Arc::clone(&metrics);
    let cancel_for_reader = cancel.clone();

    tokio::spawn(async move {
        // Callback invoked for every successfully deserialized ServerEvent
        let metrics_for_entry = Arc::clone(&metrics_reader);
        let on_entry = async move |msg: ServerEvent| -> Result<(), AlysonError> {
            metrics_for_entry
                .msgs_in
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            // If this is an echo of our own live ChatMessage probe, measure round-trip latency.
            // We verify entry.content and ensure rtt < 5s to ignore historical messages replayed from DB.
            if let ServerEvent::ChatMessage(entry) = msg {
                if entry.msg == "chatty probe" && entry.from == client_id {
                    if let Ok(rtt) = SystemTime::now().duration_since(entry.time) {
                        if rtt < Duration::from_secs(5) {
                            metrics_for_entry.record_roundtrip_latency(rtt);
                        }
                    }
                }
            }
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

        // 6. Cleanup on disconnect
        metrics_reader.record_disconnect();
    });

    Ok(client_id)
}
