use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use tokio::sync::mpsc;
use wire_chat_rs::WireError;
use wire_chat_rs::network::{NetworkHandle, run_network_engine};
use wire_chat_rs::ui::run;

#[tokio::main]
async fn main() -> Result<(), WireError> {
    // --- channels setup ---
    let (cmd_tx, cmd_rx) = mpsc::channel(100);
    let (event_tx, event_rx) = mpsc::channel(100);

    let net_handle = NetworkHandle::new(cmd_tx);

    // --- run network engine ---
    tokio::spawn(run_network_engine(cmd_rx, event_tx));

    // --- terminal setup ---
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // --- run ui ---
    // We don't use catch_unwind here because it's complicated with async,
    // but we ensure teardown runs with a result-based approach.
    let result = run(&mut terminal, net_handle, event_rx).await;

    // --- teardown (must run even on error, so terminal isn't left broken) ---
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}
