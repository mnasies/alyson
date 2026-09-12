use alyson::WireError;
use alyson::network::{NetworkHandle, run_network_engine};
use alyson::ui::run;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use tokio::sync::mpsc;

fn reset_terminal() {
    use std::io::Write;
    let _ = disable_raw_mode();
    let mut stdout = stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, crossterm::cursor::Show);
    let _ = stdout.flush();
}

#[tokio::main]
async fn main() -> Result<(), WireError> {
    // --- panic hook setup to prevent breaking terminal on panic ---
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        reset_terminal();
        original_hook(panic_info);
        // Explicitly exit the process so background panics kill the UI too
        std::process::exit(1);
    }));

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
    let result = run(&mut terminal, net_handle, event_rx).await;

    // --- teardown ---
    reset_terminal();

    result
}
