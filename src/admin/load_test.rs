use crate::admin::headless_client::{CancellationToken, Scenario, headless_connect};
use crate::admin::metrics::ServerMetrics;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Runs a load test with the specified scenario (ConnectStorm or Chatty) against a target server.
pub async fn run_load_test(
    server_addr: String,
    count: usize,
    ramp_rate: usize,
    scenario: Scenario,
    metrics: Arc<ServerMetrics>,
    cancel: CancellationToken,
) {
    let ramp_rate = ramp_rate.max(1);
    let interval = Duration::from_secs_f64(1.0 / ramp_rate as f64);
    let start_time = Instant::now();

    let scenario_name = match scenario {
        Scenario::ConnectStorm => "Connect-Storm",
        Scenario::Chatty => "Chatty (Active Messaging)",
    };

    println!(
        "⚡ Starting {}: targeting {} with {} clients at {}/sec",
        scenario_name, server_addr, count, ramp_rate
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
                        "[{:>5.1}s] active: {:<4} tot: {:<4} fail: {:<2} (fd: {}) | msgs out/in: {}/{} | conn p99: {} | rtt p99: {} | CPU: {:>4.1}% | RSS: {:>5.1}MB",
                        elapsed,
                        snap.active_connections,
                        snap.total_connects,
                        snap.connect_failures,
                        snap.fd_limit_errors,
                        snap.msgs_out,
                        snap.msgs_in,
                        snap.connect_latency_p99_str(),
                        snap.roundtrip_latency_p99_str(),
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
            println!("🛑 Load test cancelled by user.");
            break;
        }

        let username = format!("bot_{}", i);
        let addr = server_addr.clone();
        let m = Arc::clone(&metrics);
        let c = cancel.clone();

        tokio::spawn(async move {
            let _ = headless_connect(addr, username, m, c, scenario).await;
        });

        tokio::time::sleep(interval).await;
    }

    println!("✅ Finished launching {} clients. Holding test running...", count);

    // Keep running until cancellation is requested
    cancel.cancelled().await;

    let _ = reporter_handle.await;
    let snap = metrics.snapshot();
    println!(
        "\n📊 {} Finished. Summary:\n   • Active connections: {}\n   • Total connected:    {}\n   • Connect failures:   {} (fd limit: {})\n   • Messages Sent:      {}\n   • Messages Received:  {}\n   • Connect Latency:    p50: {} | p99: {}\n   • Roundtrip Latency:  p50: {} | p99: {}\n   • Process Resources:  CPU: {:.1}% | RSS: {:.1}MB",
        scenario_name,
        snap.active_connections,
        snap.total_connects,
        snap.connect_failures,
        snap.fd_limit_errors,
        snap.msgs_out,
        snap.msgs_in,
        snap.connect_latency_p50_str(),
        snap.connect_latency_p99_str(),
        snap.roundtrip_latency_p50_str(),
        snap.roundtrip_latency_p99_str(),
        snap.cpu_percent,
        snap.memory_mb()
    );
}

/// Convenience wrapper for ConnectStorm scenario.
pub async fn run_connect_storm(
    server_addr: String,
    count: usize,
    ramp_rate: usize,
    metrics: Arc<ServerMetrics>,
    cancel: CancellationToken,
) {
    run_load_test(
        server_addr,
        count,
        ramp_rate,
        Scenario::ConnectStorm,
        metrics,
        cancel,
    )
    .await;
}

/// Convenience wrapper for Chatty scenario.
pub async fn run_chatty(
    server_addr: String,
    count: usize,
    ramp_rate: usize,
    metrics: Arc<ServerMetrics>,
    cancel: CancellationToken,
) {
    run_load_test(
        server_addr,
        count,
        ramp_rate,
        Scenario::Chatty,
        metrics,
        cancel,
    )
    .await;
}
