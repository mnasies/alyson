use alyson::WireError;
use alyson::network::client_side::run_client;
use alyson::network::{NetworkHandle, run_network_engine};

use alyson::ui::run;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use tokio::sync::mpsc;

use std::env;

fn reset_terminal() {
    use std::io::Write;
    let _ = disable_raw_mode();
    let mut stdout = stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, crossterm::cursor::Show);
    let _ = stdout.flush();
}

#[tokio::main]
async fn main() -> Result<(), WireError> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    // --- channels setup ---
    let (cmd_tx, cmd_rx) = mpsc::channel(100);
    let (event_tx, event_rx) = mpsc::channel(100);
    let identity_mode; // 0: random, 1: fixed
    let mut client_name = String::new();

    match args.get(1).map(String::as_str) {
        Some("connect") => {
            let addr = args
                .get(2)
                .cloned()
                .expect("usage: client connect <ip>:<port> --name <name>");
            let name_idx = args.iter().position(|a| a == "--name");
            client_name = name_idx
                .and_then(|i| args.get(i + 1))
                .cloned()
                .expect("--name <name> is required for connect mode");
            tokio::spawn(run_client(event_tx, cmd_rx, addr));
            identity_mode = 1;
        }
        _ => {
            // dev mode: this process spawns BOTH the server task and a client task,
            tokio::spawn(run_network_engine(cmd_rx, event_tx));
            identity_mode = 0;
        }
    }

    // --- panic hook setup to prevent breaking terminal on panic ---
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        reset_terminal();
        original_hook(panic_info);
        // Explicitly exit the process so background panics kill the UI too
        std::process::exit(1);
    }));

    let net_handle = NetworkHandle::new(cmd_tx);
    // --- terminal setup ---
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // --- run ui ---
    let result = run(
        &mut terminal,
        net_handle,
        event_rx,
        identity_mode,
        client_name,
    )
    .await;

    // --- teardown ---
    reset_terminal();

    result
}
