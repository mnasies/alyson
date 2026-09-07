use alyson::network::{NetworkCommand, NetworkEvent, run_network_engine};

#[tokio::test]
async fn test_spawn_client() {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(100);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

    tokio::spawn(run_network_engine(cmd_rx, event_tx));

    cmd_tx
        .send(NetworkCommand::SpawnClient {
            username: "Alice".to_string(),
        })
        .await
        .unwrap();

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("channel closed");

    match event {
        NetworkEvent::ClientConnected { username, .. } => {
            assert_eq!(username, "Alice");
        }
        other => panic!("Unexpected event: {:?}", other),
    }
}

#[tokio::test]
async fn test_spawn_multiple_clients() {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(100);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

    tokio::spawn(run_network_engine(cmd_rx, event_tx));

    for i in 0..5 {
        cmd_tx
            .send(NetworkCommand::SpawnClient {
                username: format!("Client{}", i),
            })
            .await
            .unwrap();
    }

    let mut connected_count = 0;
    for _ in 0..5 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("channel closed");
        if let NetworkEvent::ClientConnected { .. } = event {
            connected_count += 1;
        }
    }
    assert_eq!(connected_count, 5);

    // Regression guard: no immediate ClientDisconnected should be emitted,
    // because the client-side writer is kept alive for the connection's lifetime.
    let next = tokio::time::timeout(std::time::Duration::from_millis(200), event_rx.recv()).await;
    assert!(
        next.is_err() || matches!(next, Ok(None)),
        "expected no immediate ClientDisconnected event, got: {:?}",
        next
    );
}
