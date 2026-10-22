use alyson::admin::headless_client::{CancellationToken, Scenario, headless_connect};
use alyson::admin::load_test::run_connect_storm;
use alyson::admin::metrics::{ServerMetrics, aggregate_snapshots};
use alyson::admin::server_manager::{ServerStatus, start_server};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_server_lifecycle() {
    // 1. Start a child server process on an OS-assigned ephemeral port ("127.0.0.1:0")
    let mut server = start_server(0, "127.0.0.1:0").expect("Failed to start server child");

    assert_eq!(server.id, 0);
    assert!(server.pid > 0, "Server PID must be positive");
    assert!(
        server.addr.starts_with("127.0.0.1:"),
        "Bound address must start with 127.0.0.1:, got: {}",
        server.addr
    );
    assert_eq!(server.status, ServerStatus::Running);

    // 2. Stop the server child process gracefully
    server.stop().expect("Failed to stop server");
    assert_eq!(server.status, ServerStatus::Stopped);

    // Polling check_status should also confirm it remains Stopped
    assert_eq!(server.check_status(), ServerStatus::Stopped);
}

#[tokio::test]
async fn test_headless_client_connect_and_metrics() {
    let mut server = start_server(1, "127.0.0.1:0").expect("Failed to start server");
    let metrics = Arc::new(ServerMetrics::new());
    let cancel = CancellationToken::new();

    // Connect a single headless client
    let client_id = headless_connect(
        server.addr.clone(),
        "test_bot_1".to_string(),
        Arc::clone(&metrics),
        cancel.clone(),
        Scenario::ConnectStorm,
    )
    .await
    .expect("Headless client failed to connect and handshake");

    assert!(client_id > 0, "Assigned client ID must be > 0");

    // Verify metrics show 1 active and 1 total connect
    let (active, total, failed) = metrics.connection_summary();
    assert_eq!(active, 1);
    assert_eq!(total, 1);
    assert_eq!(failed, 0);

    // Cancel token to disconnect client
    cancel.cancel();

    // Give tokio task a moment to process disconnect and decrement active counter
    tokio::time::sleep(Duration::from_millis(100)).await;

    let (active_after, _, _) = metrics.connection_summary();
    assert_eq!(active_after, 0);

    server.stop().expect("Failed to stop server");
}

#[tokio::test]
async fn test_connect_storm_batch() {
    let mut server = start_server(2, "127.0.0.1:0").expect("Failed to start server");
    let metrics = Arc::new(ServerMetrics::new());
    let cancel = CancellationToken::new();

    let storm_addr = server.addr.clone();
    let storm_metrics = Arc::clone(&metrics);
    let storm_cancel = cancel.clone();

    // Run a small connect-storm of 10 clients at 50/sec in a background task
    let storm_task = tokio::spawn(async move {
        run_connect_storm(storm_addr, 10, 50, storm_metrics, storm_cancel).await;
    });

    // Wait until 10 connections have been registered
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while metrics.total_connects.load(std::sync::atomic::Ordering::Relaxed) < 10 {
        if std::time::Instant::now() > deadline {
            panic!("Timed out waiting for 10 clients to connect in storm");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let (active, total, failed) = metrics.connection_summary();
    assert_eq!(total, 10);
    assert_eq!(active, 10);
    assert_eq!(failed, 0);

    // Cancel storm
    cancel.cancel();
    let _ = storm_task.await;

    // Wait for disconnects
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (active_after, _, _) = metrics.connection_summary();
    assert_eq!(active_after, 0);

    server.stop().expect("Failed to stop server");
}

#[tokio::test]
async fn test_histogram_latency_percentiles() {
    let metrics = ServerMetrics::new();

    // Record synthetic latencies: 100 values from 1ms to 100ms
    for ms in 1..=100 {
        metrics.record_connect_latency(Duration::from_millis(ms));
    }

    let snap = metrics.snapshot();
    // p50 should be around 50ms (50,000 micros)
    let p50_ms = snap.connect_latency_p50_us as f64 / 1000.0;
    assert!(
        (48.0..=52.0).contains(&p50_ms),
        "Expected p50 around 50ms, got: {:.2}ms",
        p50_ms
    );

    // p99 should be around 99ms (99,000 micros)
    let p99_ms = snap.connect_latency_p99_us as f64 / 1000.0;
    assert!(
        (97.0..=100.0).contains(&p99_ms),
        "Expected p99 around 99ms, got: {:.2}ms",
        p99_ms
    );
}

#[tokio::test]
async fn test_rolling_history_buffer() {
    use alyson::admin::RollingHistory;

    let mut history = RollingHistory::new(3);
    assert_eq!(history.entries.len(), 0);

    let metrics = ServerMetrics::new();

    // Push 4 snapshots to verify max_entries eviction
    for _ in 0..4 {
        metrics.record_connect();
        history.push(metrics.snapshot());
    }

    assert_eq!(history.entries.len(), 3, "History must cap at max_entries (3)");
    let latest = history.latest().expect("Must have latest entry");
    assert_eq!(latest.total_connects, 4);
}

#[tokio::test]
async fn test_sysinfo_sampler_process_stats() {
    // Start a real child server process
    let mut server = start_server(100, "127.0.0.1:0").expect("Failed to start server");

    // Wait up to 3 seconds for the sysinfo sampler task to refresh memory
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut observed_memory = false;

    while std::time::Instant::now() < deadline {
        let snap = server.metrics.snapshot();
        if snap.memory_bytes > 0 {
            observed_memory = true;
            assert!(
                snap.memory_mb() > 0.1,
                "Server RSS memory must be positive, got: {} MB",
                snap.memory_mb()
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert!(
        observed_memory,
        "sysinfo sampler should have recorded process memory > 0 bytes within 3s"
    );

    server.stop().expect("Failed to stop server");
}

#[tokio::test]
async fn test_admin_app_lifecycle_and_selection() {
    use alyson::admin::AdminApp;

    let mut app = AdminApp::new();
    assert!(app.selected_server().is_none());

    // Start server 0
    app.start_server("127.0.0.1:0");
    assert_eq!(app.server_manager.servers().len(), 1);
    assert_eq!(app.selected_index, 0);

    let s0_id = app.selected_server().map(|s| s.id).unwrap();
    assert_eq!(s0_id, 0);

    // Start server 1
    app.start_server("127.0.0.1:0");
    assert_eq!(app.server_manager.servers().len(), 2);
    assert_eq!(app.selected_index, 1);

    // Test selection navigation
    app.select_prev();
    assert_eq!(app.selected_index, 0);
    app.select_next();
    assert_eq!(app.selected_index, 1);

    // Stop selected server
    app.stop_selected_server();
    let s1_status = app.selected_server().map(|s| s.status.clone()).unwrap();
    assert_eq!(s1_status, ServerStatus::Stopped);

    app.server_manager.stop_all();
}

#[tokio::test]
async fn test_admin_app_batch_and_snapshots() {
    use alyson::admin::AdminApp;

    let mut app = AdminApp::new();
    app.start_server("127.0.0.1:0");

    let server_id = app.selected_server().map(|s| s.id).unwrap();

    // Launch 3 headless clients to this server
    app.batch_launch_selected(3, 50, Scenario::ConnectStorm);

    // Wait for connections to establish and tick snapshots
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        app.last_snapshot_tick = std::time::Instant::now() - Duration::from_millis(300);
        app.tick_snapshots();
        if let Some(history) = app.history.get(&server_id) {
            if let Some(latest) = history.latest() {
                if latest.total_connects == 3 && latest.active_connections == 3 {
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let history = app.history.get(&server_id).expect("History must exist for server");
    assert!(!history.entries.is_empty(), "Snapshot must be recorded in history");
    let latest = history.latest().unwrap();
    assert_eq!(latest.total_connects, 3);
    assert_eq!(latest.active_connections, 3);

    // Disconnect clients
    app.disconnect_clients(server_id);

    let disconnect_deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < disconnect_deadline {
        app.last_snapshot_tick = std::time::Instant::now() - Duration::from_millis(300);
        app.tick_snapshots();
        if let Some(history) = app.history.get(&server_id) {
            if let Some(latest) = history.latest() {
                if latest.active_connections == 0 {
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let latest_after = app.history.get(&server_id).unwrap().latest().unwrap();
    assert_eq!(latest_after.active_connections, 0);

    app.server_manager.stop_all();
}

#[tokio::test]
async fn test_chatty_scenario_rtt() {
    let mut server = start_server(10, "127.0.0.1:0").expect("Failed to start server");
    let metrics = Arc::new(ServerMetrics::new());
    let cancel = CancellationToken::new();

    // Connect a chatty client (sends periodic chat pings and awaits echo)
    let client_id = headless_connect(
        server.addr.clone(),
        "chatty_bot".to_string(),
        Arc::clone(&metrics),
        cancel.clone(),
        Scenario::Chatty,
    )
    .await
    .expect("Chatty client failed to connect");

    assert!(client_id > 0);

    // Wait until echo is returned and roundtrip latency is recorded
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut recorded_rtt = false;

    while std::time::Instant::now() < deadline {
        let snap = metrics.snapshot();
        if snap.roundtrip_latency_p50_us > 0 {
            recorded_rtt = true;
            assert!(snap.msgs_out > 0);
            assert!(snap.msgs_in > 0);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(recorded_rtt, "Chatty client should send and receive chat messages within 3s");

    cancel.cancel();
    server.stop().expect("Failed to stop server");
}

#[tokio::test]
async fn test_aggregate_snapshots() {
    let m1 = ServerMetrics::new();
    let m2 = ServerMetrics::new();

    m1.record_connect();
    m1.record_connect();
    m1.record_connect_latency(Duration::from_millis(10));
    m1.record_msg_out();
    m1.record_msg_in();

    m2.record_connect();
    m2.record_connect_failure();
    m2.record_fd_limit_error(); // EMFILE fd limit error
    m2.record_connect_latency(Duration::from_millis(50));
    m2.record_msg_out();

    let snap1 = m1.snapshot();
    let snap2 = m2.snapshot();

    let agg = aggregate_snapshots(&[snap1, snap2]);

    assert_eq!(agg.total_connects, 3);
    assert_eq!(agg.active_connections, 3);
    assert_eq!(agg.connect_failures, 2);
    assert_eq!(agg.fd_limit_errors, 1);
    assert_eq!(agg.msgs_out, 2);
    assert_eq!(agg.msgs_in, 1);

    // Max latency p99 across servers should reflect the higher one (~50ms)
    assert!(agg.connect_latency_p99_us >= 40_000);
}

#[tokio::test]
async fn test_multi_server_batch_and_aggregate() {
    use alyson::admin::AdminApp;

    let mut app = AdminApp::new();

    // Start 2 servers
    app.start_server("127.0.0.1:0");
    app.start_server("127.0.0.1:0");
    assert_eq!(app.server_manager.servers().len(), 2);

    // Distribute 4 clients across all running servers
    app.batch_launch_all(4, 50, Scenario::ConnectStorm);

    // Wait for connections to establish across all servers
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        let agg = app.aggregate_snapshot();
        if agg.active_connections == 4 && agg.total_connects == 4 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // In aggregate mode, active connections across both servers should equal 4
    let agg = app.aggregate_snapshot();
    assert_eq!(agg.active_connections, 4);
    assert_eq!(agg.total_connects, 4);

    // Toggle aggregate mode
    app.view_aggregate = true;
    assert!(app.view_aggregate);

    app.server_manager.stop_all();
}

#[tokio::test]
async fn test_fd_limit_error_handling() {
    let metrics = ServerMetrics::new();
    assert_eq!(metrics.fd_limit_errors.load(std::sync::atomic::Ordering::Relaxed), 0);

    // Simulate standard connection failure
    metrics.record_connect_failure();
    assert_eq!(metrics.connect_failures.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(metrics.fd_limit_errors.load(std::sync::atomic::Ordering::Relaxed), 0);

    // Simulate EMFILE / ENFILE OS file descriptor exhaustion
    metrics.record_fd_limit_error();
    assert_eq!(metrics.connect_failures.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(metrics.fd_limit_errors.load(std::sync::atomic::Ordering::Relaxed), 1);

    let snap = metrics.snapshot();
    assert_eq!(snap.fd_limit_errors, 1);
    assert_eq!(snap.connect_failures, 2);
}
