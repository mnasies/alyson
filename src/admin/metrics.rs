use hdrhistogram::Histogram;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Metrics tracked for a single managed server.
///
/// # Why Atomics + Histograms?
/// 1. Simple counters (`active_connections`, `total_connects`, `msgs_in`, etc.) use `AtomicU64`.
///    They are bumped on high-throughput hot paths using `Ordering::Relaxed` (a single hardware instruction).
/// 2. Percentile latencies (p50, p99) use `hdrhistogram::Histogram` behind a `Mutex`.
///    Because recording into an HdrHistogram takes ~50ns, lock contention is negligible,
///    while giving exact mathematical percentile guarantees across orders of magnitude.
/// 3. CPU and Memory are updated periodically by a background sampler task. CPU percentage is stored
///    as a fixed-point integer (e.g. `1234` represents `12.34%`) to keep it atomic without floating-point atomics.
pub struct ServerMetrics {
    /// Number of TCP connections currently open and alive.
    pub active_connections: AtomicU64,

    /// Lifetime total number of successful client TCP connections established.
    pub total_connects: AtomicU64,

    /// Lifetime total number of failed connection attempts (e.g. ECONNREFUSED, timeout, fd limits).
    pub connect_failures: AtomicU64,

    /// Number of failures specifically caused by reaching OS file descriptor limits (EMFILE / ENFILE).
    pub fd_limit_errors: AtomicU64,

    /// Total messages received from the server.
    pub msgs_in: AtomicU64,

    /// Total messages sent to the server.
    pub msgs_out: AtomicU64,

    /// Total bytes read across all client connections.
    pub bytes_in: AtomicU64,

    /// Total bytes written across all client connections.
    pub bytes_out: AtomicU64,

    /// Histogram recording TCP connect + handshake latency in microseconds.
    /// Tracks values from 1µs to 60,000,000µs (60s) with 3 significant figures of precision.
    pub connect_latency: Mutex<Histogram<u64>>,

    /// Histogram recording request-to-response round-trip latency in microseconds.
    pub roundtrip_latency: Mutex<Histogram<u64>>,

    /// CPU utilization of the child server process, stored as `percent * 100` (e.g. 2550 = 25.50%).
    pub cpu_percent: AtomicU64,

    /// Resident Set Size (RSS) memory of the child server process in bytes.
    pub memory_bytes: AtomicU64,
}

impl std::fmt::Debug for ServerMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerMetrics")
            .field("active_connections", &self.active_connections.load(Ordering::Relaxed))
            .field("total_connects", &self.total_connects.load(Ordering::Relaxed))
            .field("connect_failures", &self.connect_failures.load(Ordering::Relaxed))
            .field("msgs_in", &self.msgs_in.load(Ordering::Relaxed))
            .field("msgs_out", &self.msgs_out.load(Ordering::Relaxed))
            .field("cpu_percent", &(self.cpu_percent.load(Ordering::Relaxed) as f64 / 100.0))
            .field("memory_bytes", &self.memory_bytes.load(Ordering::Relaxed))
            .finish()
    }
}

impl Default for ServerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerMetrics {
    /// Create a new, zero-initialized metrics container with pre-allocated histograms.
    pub fn new() -> Self {
        // Range: 1 microsecond to 60 seconds (60,000,000 micros), 3 significant digits
        let connect_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)
            .expect("Valid histogram bounds for connect_latency");
        let rtt_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)
            .expect("Valid histogram bounds for roundtrip_latency");

        Self {
            active_connections: AtomicU64::new(0),
            total_connects: AtomicU64::new(0),
            connect_failures: AtomicU64::new(0),
            fd_limit_errors: AtomicU64::new(0),
            msgs_in: AtomicU64::new(0),
            msgs_out: AtomicU64::new(0),
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            connect_latency: Mutex::new(connect_hist),
            roundtrip_latency: Mutex::new(rtt_hist),
            cpu_percent: AtomicU64::new(0),
            memory_bytes: AtomicU64::new(0),
        }
    }

    /// Record a newly established client connection.
    pub fn record_connect(&self) {
        self.total_connects.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that an active client connection has closed or dropped.
    pub fn record_disconnect(&self) {
        let _ = self
            .active_connections
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |curr| {
                Some(curr.saturating_sub(1))
            });
    }

    /// Record a failed connection attempt (server down, port closed, etc.).
    pub fn record_connect_failure(&self) {
        self.connect_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failure specifically triggered by OS file descriptor exhaustion (EMFILE / ENFILE).
    pub fn record_fd_limit_error(&self) {
        self.fd_limit_errors.fetch_add(1, Ordering::Relaxed);
        self.connect_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an outgoing message sent over the wire.
    pub fn record_msg_out(&self) {
        self.msgs_out.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an incoming message received over the wire.
    pub fn record_msg_in(&self) {
        self.msgs_in.fetch_add(1, Ordering::Relaxed);
    }

    /// Record the duration of a TCP connect and initial handshake.
    pub fn record_connect_latency(&self, duration: Duration) {
        let micros = (duration.as_micros() as u64).clamp(1, 60_000_000);
        if let Ok(mut hist) = self.connect_latency.lock() {
            let _ = hist.record(micros);
        }
    }

    /// Record a message round-trip time.
    pub fn record_roundtrip_latency(&self, duration: Duration) {
        let micros = (duration.as_micros() as u64).clamp(1, 60_000_000);
        if let Ok(mut hist) = self.roundtrip_latency.lock() {
            let _ = hist.record(micros);
        }
    }

    /// Update OS-level CPU and memory measurements.
    pub fn update_process_stats(&self, cpu: f64, memory_bytes: u64) {
        let cpu_fixed = (cpu * 100.0).clamp(0.0, 100_000.0) as u64;
        self.cpu_percent.store(cpu_fixed, Ordering::Relaxed);
        self.memory_bytes.store(memory_bytes, Ordering::Relaxed);
    }

    /// Fetch a quick point-in-time copy of primary connection counters.
    pub fn connection_summary(&self) -> (u64, u64, u64) {
        (
            self.active_connections.load(Ordering::Relaxed),
            self.total_connects.load(Ordering::Relaxed),
            self.connect_failures.load(Ordering::Relaxed),
        )
    }

    /// Capture an immutable point-in-time snapshot of all metrics.
    ///
    /// The TUI and dashboard widgets consume this struct instead of reading atomics
    /// or locking histograms directly. This completely decouples UI rendering from
    /// the performance of worker tasks.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let (connect_p50, connect_p99) = {
            if let Ok(hist) = self.connect_latency.lock() {
                if hist.len() > 0 {
                    (hist.value_at_quantile(0.50), hist.value_at_quantile(0.99))
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            }
        };

        let (rtt_p50, rtt_p99) = {
            if let Ok(hist) = self.roundtrip_latency.lock() {
                if hist.len() > 0 {
                    (hist.value_at_quantile(0.50), hist.value_at_quantile(0.99))
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            }
        };

        let cpu_raw = self.cpu_percent.load(Ordering::Relaxed);
        let cpu_percent = cpu_raw as f64 / 100.0;

        MetricsSnapshot {
            active_connections: self.active_connections.load(Ordering::Relaxed),
            total_connects: self.total_connects.load(Ordering::Relaxed),
            connect_failures: self.connect_failures.load(Ordering::Relaxed),
            fd_limit_errors: self.fd_limit_errors.load(Ordering::Relaxed),
            msgs_in: self.msgs_in.load(Ordering::Relaxed),
            msgs_out: self.msgs_out.load(Ordering::Relaxed),
            bytes_in: self.bytes_in.load(Ordering::Relaxed),
            bytes_out: self.bytes_out.load(Ordering::Relaxed),
            connect_latency_p50_us: connect_p50,
            connect_latency_p99_us: connect_p99,
            roundtrip_latency_p50_us: rtt_p50,
            roundtrip_latency_p99_us: rtt_p99,
            cpu_percent,
            memory_bytes: self.memory_bytes.load(Ordering::Relaxed),
            timestamp: Instant::now(),
        }
    }
}

/// A frozen, thread-safe point-in-time snapshot of a server's metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricsSnapshot {
    pub active_connections: u64,
    pub total_connects: u64,
    pub connect_failures: u64,
    pub fd_limit_errors: u64,
    pub msgs_in: u64,
    pub msgs_out: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    /// 50th percentile (median) connection latency in microseconds.
    pub connect_latency_p50_us: u64,
    /// 99th percentile connection latency in microseconds.
    pub connect_latency_p99_us: u64,
    /// 50th percentile round-trip latency in microseconds.
    pub roundtrip_latency_p50_us: u64,
    /// 99th percentile round-trip latency in microseconds.
    pub roundtrip_latency_p99_us: u64,
    /// CPU utilization percentage (e.g. 15.4%).
    pub cpu_percent: f64,
    /// Resident memory in bytes.
    pub memory_bytes: u64,
    /// Timestamp when this snapshot was taken.
    pub timestamp: Instant,
}

impl MetricsSnapshot {
    /// Format connection latency p50 into a friendly human string (µs or ms).
    pub fn connect_latency_p50_str(&self) -> String {
        format_micros(self.connect_latency_p50_us)
    }

    /// Format connection latency p99 into a friendly human string (µs or ms).
    pub fn connect_latency_p99_str(&self) -> String {
        format_micros(self.connect_latency_p99_us)
    }

    /// Format round-trip latency p50 into a friendly human string (µs or ms).
    pub fn roundtrip_latency_p50_str(&self) -> String {
        format_micros(self.roundtrip_latency_p50_us)
    }

    /// Format round-trip latency p99 into a friendly human string (µs or ms).
    pub fn roundtrip_latency_p99_str(&self) -> String {
        format_micros(self.roundtrip_latency_p99_us)
    }

    /// Format memory into Megabytes.
    pub fn memory_mb(&self) -> f64 {
        self.memory_bytes as f64 / (1024.0 * 1024.0)
    }
}

/// Aggregate metrics snapshots across multiple servers into a unified summary.
pub fn aggregate_snapshots(snapshots: &[MetricsSnapshot]) -> MetricsSnapshot {
    if snapshots.is_empty() {
        return MetricsSnapshot {
            active_connections: 0,
            total_connects: 0,
            connect_failures: 0,
            fd_limit_errors: 0,
            msgs_in: 0,
            msgs_out: 0,
            bytes_in: 0,
            bytes_out: 0,
            connect_latency_p50_us: 0,
            connect_latency_p99_us: 0,
            roundtrip_latency_p50_us: 0,
            roundtrip_latency_p99_us: 0,
            cpu_percent: 0.0,
            memory_bytes: 0,
            timestamp: Instant::now(),
        };
    }

    let mut total_active = 0;
    let mut total_connects = 0;
    let mut total_failures = 0;
    let mut total_fd_errors = 0;
    let mut total_msgs_in = 0;
    let mut total_msgs_out = 0;
    let mut total_bytes_in = 0;
    let mut total_bytes_out = 0;
    let mut total_cpu = 0.0;
    let mut total_memory = 0;
    let mut max_connect_p50 = 0;
    let mut max_connect_p99 = 0;
    let mut max_rtt_p50 = 0;
    let mut max_rtt_p99 = 0;

    for s in snapshots {
        total_active += s.active_connections;
        total_connects += s.total_connects;
        total_failures += s.connect_failures;
        total_fd_errors += s.fd_limit_errors;
        total_msgs_in += s.msgs_in;
        total_msgs_out += s.msgs_out;
        total_bytes_in += s.bytes_in;
        total_bytes_out += s.bytes_out;
        total_cpu += s.cpu_percent;
        total_memory += s.memory_bytes;
        max_connect_p50 = max_connect_p50.max(s.connect_latency_p50_us);
        max_connect_p99 = max_connect_p99.max(s.connect_latency_p99_us);
        max_rtt_p50 = max_rtt_p50.max(s.roundtrip_latency_p50_us);
        max_rtt_p99 = max_rtt_p99.max(s.roundtrip_latency_p99_us);
    }

    MetricsSnapshot {
        active_connections: total_active,
        total_connects,
        connect_failures: total_failures,
        fd_limit_errors: total_fd_errors,
        msgs_in: total_msgs_in,
        msgs_out: total_msgs_out,
        bytes_in: total_bytes_in,
        bytes_out: total_bytes_out,
        connect_latency_p50_us: max_connect_p50,
        connect_latency_p99_us: max_connect_p99,
        roundtrip_latency_p50_us: max_rtt_p50,
        roundtrip_latency_p99_us: max_rtt_p99,
        cpu_percent: total_cpu,
        memory_bytes: total_memory,
        timestamp: Instant::now(),
    }
}


fn format_micros(micros: u64) -> String {
    if micros == 0 {
        "0.0ms".to_string()
    } else if micros < 1000 {
        format!("{}µs", micros)
    } else {
        format!("{:.2}ms", micros as f64 / 1000.0)
    }
}

/// Fixed-size circular buffer maintaining chronological snapshots for TUI graphs.
///
/// At 250ms per snapshot tick, 120 entries covers the last 30 seconds of execution.
#[derive(Debug, Clone)]
pub struct RollingHistory {
    pub entries: VecDeque<MetricsSnapshot>,
    pub max_entries: usize,
}

impl Default for RollingHistory {
    fn default() -> Self {
        Self::new(120)
    }
}

impl RollingHistory {
    /// Create a new rolling history buffer with the given capacity limit.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    /// Push a new snapshot into the buffer, dropping the oldest snapshot if full.
    pub fn push(&mut self, snapshot: MetricsSnapshot) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(snapshot);
    }

    /// Get the most recently recorded snapshot.
    pub fn latest(&self) -> Option<&MetricsSnapshot> {
        self.entries.back()
    }

    /// Return an iterator over all snapshots in chronological order.
    pub fn iter(&self) -> impl Iterator<Item = &MetricsSnapshot> {
        self.entries.iter()
    }

    /// Compute delta connection rate (connections established per second) between the
    /// latest two snapshots.
    pub fn connect_rate(&self) -> f64 {
        let len = self.entries.len();
        if len < 2 {
            return 0.0;
        }
        let prev = &self.entries[len - 2];
        let curr = &self.entries[len - 1];

        let elapsed = curr.timestamp.duration_since(prev.timestamp).as_secs_f64();
        if elapsed <= 0.0 {
            return 0.0;
        }

        let delta = curr.total_connects.saturating_sub(prev.total_connects) as f64;
        delta / elapsed
    }
}
