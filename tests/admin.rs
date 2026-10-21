use alyson::admin::headless_client::{CancellationToken, headless_connect};
use alyson::admin::load_test::run_connect_storm;
use alyson::admin::metrics::ServerMetrics;
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
