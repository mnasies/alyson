use crate::admin::headless_client::{CancellationToken, headless_connect};
use crate::admin::metrics::ServerMetrics;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Runs a Connect-Storm load test scenario against a target server.
///
/// # What is a Connect-Storm?
/// A connect-storm tests how well a server's listener queue (`SOMAXCONN`), accept loop,
/// and connection handshake handle a high arrival rate of new TCP connections.
///
/// Each client:
/// 1. Connects to `server_addr`.
/// 2. Performs the initial handshake (username exchange).
/// 3. Holds the connection open to simulate steady active sessions.
///
/// # Parameters:
/// - `server_addr`: Socket address of the target server (e.g., "127.0.0.1:9000").
/// - `count`: Total number of clients to launch.
/// - `ramp_rate`: Maximum number of new clients launched per second.
/// - `metrics`: Shared atomic metrics container tracking connections and errors.
/// - `cancel`: Cooperative cancellation token to stop the test on demand.
pub async fn run_connect_storm(
    server_addr: String,
    count: usize,
    ramp_rate: usize,
    metrics: Arc<ServerMetrics>,
    cancel: CancellationToken,
) {
    let ramp_rate = ramp_rate.max(1);
    let interval = Duration::from_secs_f64(1.0 / ramp_rate as f64);
    let start_time = Instant::now();

    println!(
        "⚡ Starting Connect-Storm: targeting {} with {} clients at {}/sec",
        server_addr, count, ramp_rate
    );

    // Spawn a reporter task that prints progress every 1 second
    let reporter_metrics = Arc::clone(&metrics);
    let reporter_cancel = cancel.clone();
    let reporter_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        // Skip first immediate tick
        ticker.tick().await;

        while !reporter_cancel.is_cancelled() {
            tokio::select! {
                _ = ticker.tick() => {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let snap = reporter_metrics.snapshot();
                    println!(
                        "[{:>5.1}s] active: {:<5} total: {:<5} fail: {:<3} | latency p50: {:<7} p99: {:<7} | CPU: {:>4.1}% | RSS: {:>5.1}MB",
                        elapsed,
                        snap.active_connections,
                        snap.total_connects,
                        snap.connect_failures,
                        snap.connect_latency_p50_str(),
                        snap.connect_latency_p99_str(),
                        snap.cpu_percent,
                        snap.memory_mb()
                    );
                }
                _ = reporter_cancel.cancelled() => {
                    break;
                }
            }
        }
    });

    // Main ramp-up loop: space client creations by interval
    for i in 0..count {
        if cancel.is_cancelled() {
            println!("🛑 Connect-Storm cancelled by user.");
            break;
        }

        let username = format!("bot_{}", i);
        let addr = server_addr.clone();
        let m = Arc::clone(&metrics);
        let c = cancel.clone();

        tokio::spawn(async move {
            let _ = headless_connect(addr, username, m, c).await;
        });

        tokio::time::sleep(interval).await;
    }

    println!("✅ Finished launching {} clients. Holding connections open...", count);

    // Keep running until cancellation is requested
    cancel.cancelled().await;

    let _ = reporter_handle.await;
    let snap = metrics.snapshot();
    println!(
        "\n📊 Storm finished. Summary:\n   • Active connections: {}\n   • Total connected:    {}\n   • Connect failures:   {}\n   • Connect Latency:    p50: {} | p99: {}\n   • Process Resources:  CPU: {:.1}% | RSS: {:.1}MB",
        snap.active_connections,
        snap.total_connects,
        snap.connect_failures,
        snap.connect_latency_p50_str(),
        snap.connect_latency_p99_str(),
        snap.cpu_percent,
        snap.memory_mb()
    );
}
