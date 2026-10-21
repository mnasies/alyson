use std::sync::atomic::{AtomicU64, Ordering};

/// Metrics tracked for a single managed server.
///
/// # Why Atomics?
/// In a high-load scenario (e.g. hundreds or thousands of clients connecting/sending messages),
/// having tasks send metric events over channels would introduce allocation, channel contention,
/// and latency.
///
/// An atomic instruction like `fetch_add(1, Ordering::Relaxed)` maps to a single hardware CPU instruction
/// without locks or OS context switches. `Ordering::Relaxed` is sufficient here because we only care
/// about the monotonicity and eventual accuracy of the individual counter, not synchronization
/// with other memory writes.
#[derive(Debug, Default)]
pub struct ServerMetrics {
    /// Number of TCP connections currently open and alive.
    /// Incremented on handshake success, decremented on socket close/disconnect.
    pub active_connections: AtomicU64,

    /// Lifetime total number of successful client TCP connections established.
    pub total_connects: AtomicU64,

    /// Lifetime total number of failed connection attempts (e.g. ECONNREFUSED, timeout, fd limits).
    pub connect_failures: AtomicU64,

    /// Total messages received from the server.
    pub msgs_in: AtomicU64,

    /// Total messages sent to the server.
    pub msgs_out: AtomicU64,

    /// Total bytes read across all client connections.
    pub bytes_in: AtomicU64,

    /// Total bytes written across all client connections.
    pub bytes_out: AtomicU64,
}

impl ServerMetrics {
    /// Create a new, zero-initialized metrics container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a newly established client connection.
    pub fn record_connect(&self) {
        self.total_connects.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that an active client connection has closed or dropped.
    pub fn record_disconnect(&self) {
        // Saturating subtraction to guard against underflow in abnormal disconnect order.
        let _ = self
            .active_connections
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |curr| {
                Some(curr.saturating_sub(1))
            });
    }

    /// Record a failed connection attempt (server down, port closed, EMFILE, etc.).
    pub fn record_connect_failure(&self) {
        self.connect_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Fetch a point-in-time copy of the primary connection counters.
    pub fn connection_summary(&self) -> (u64, u64, u64) {
        (
            self.active_connections.load(Ordering::Relaxed),
            self.total_connects.load(Ordering::Relaxed),
            self.connect_failures.load(Ordering::Relaxed),
        )
    }
}
