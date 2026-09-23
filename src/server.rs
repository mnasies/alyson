use alyson::WireError;
use alyson::network::{NetworkHandle, run_server};

#[tokio::main]
async fn main() -> Result<(), WireError> {
    let (event_tx, event_rx) = mpsc::channel(100);

    let net_handle = NetworkHandle::new(cmd_tx);

    // --- run network engine ---
    tokio::spawn(run_server(cmd_rx, event_tx));

    // --- terminal setup ---
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // --- run ui ---
    let result = run(&mut terminal, net_handle, event_rx).await;

    // --- teardown ---
    reset_terminal();

    result
}
