use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use wire_chat_rs::WireError;
use wire_chat_rs::ui::run;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use wire_chat_rs::client::Client;
use wire_chat_rs::network::NetworkHandle;

fn main() -> Result<(), WireError> {
    let net_handle = NetworkHandle::new();
    let mut net_handle_clone = net_handle.clone();
    thread::spawn(move || {
        net_handle_clone.run_server().unwrap();
    });

    // --- setup ---
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // --- run app ---
    let ui_clients = Arc::clone(&clients);
    let net_handle_clone_ui = net_handle.clone();
    let ui_errors = Arc::clone(&errors);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(&mut terminal, net_handle_clone_ui)
    }));

    // --- teardown (must run even on error, so terminal isn't left broken) ---
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    match result {
        Ok(r) => r,
        Err(err) => {
            eprintln!("app panicked: {:?}", err);
            Ok(())
        }
    }
}
