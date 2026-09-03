use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use wire_chat_rs::network;
use wire_chat_rs::ui::run;

use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use std::thread;
use wire_chat_rs::client::Client;
use wire_chat_rs::network::NetworkHandle;

fn main() -> std::io::Result<()> {
    let clients = Arc::new(Mutex::new(Vec::<Client>::new()));

    let server_clients = Arc::clone(&clients);
    thread::spawn(move || {
        let main_network_handle = NetworkHandle::new(server_clients);
        main_network_handle.run_server().unwrap();
    });

    // --- setup ---
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // --- run app ---
    let ui_clients = Arc::clone(&clients);
    let result = run(&mut terminal, ui_clients);

    // --- teardown (must run even on error, so terminal isn't left broken) ---
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}
