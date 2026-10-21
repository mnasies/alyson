pub mod app;
pub mod dashboard;

pub use app::{AdminApp, InputMode};
pub use dashboard::render_dashboard;

use crate::AlysonError;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use std::time::Duration;

/// Restores the terminal to its normal cooked mode and clears alternate screens.
pub fn reset_terminal() {
    use std::io::Write;
    let _ = disable_raw_mode();
    let mut stdout = stdout();
    let _ = execute!(stdout, LeaveAlternateScreen, crossterm::cursor::Show);
    let _ = stdout.flush();
}

/// Launches the interactive Ratatui dashboard for Alyson Admin.
///
/// Sets up raw terminal mode, custom panic hook for crash safety,
/// event polling loop at 60fps, and 250ms snapshot metric aggregation.
pub async fn run_admin_tui() -> Result<(), AlysonError> {
    // 1. Setup panic hook to guarantee terminal restoration if a panic occurs
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        reset_terminal();
        original_hook(panic_info);
        std::process::exit(1);
    }));

    // 2. Setup terminal raw mode & alternate screen
    enable_raw_mode()?;
    let mut stdout_handle = stdout();
    execute!(stdout_handle, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout_handle);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AdminApp::new();

    // 3. Main event loop
    let res = run_loop(&mut terminal, &mut app).await;

    // 4. Teardown
    app.server_manager.stop_all();
    reset_terminal();

    res
}

async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut AdminApp,
) -> Result<(), AlysonError> {
    loop {
        // Step A: Refresh metric snapshots every 250ms
        app.tick_snapshots();

        // Step B: Draw UI frame
        terminal.draw(|f| render_dashboard(f, app))?;

        if app.should_quit {
            break;
        }

        // Step C: Poll for user key events with ~16ms timeout (~60fps)
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key_event(app, key.code);
                }
            }
        }
    }

    Ok(())
}

fn handle_key_event(app: &mut AdminApp, code: KeyCode) {
    match app.input_mode {
        InputMode::Normal => match code {
            KeyCode::Char('s') => {
                app.input_mode = InputMode::PromptStartServer;
                app.input_buffer.clear();
            }
            KeyCode::Char('x') => {
                app.stop_selected_server();
            }
            KeyCode::Char('b') => {
                if app.selected_server().is_some() {
                    app.input_mode = InputMode::PromptBatchLaunch;
                    app.input_buffer.clear();
                } else {
                    app.set_toast("Select a server first (press 's' to start one).".to_string(), true);
                }
            }
            KeyCode::Char('d') => {
                let id_opt = app.selected_server().map(|s| s.id);
                if let Some(id) = id_opt {
                    app.disconnect_clients(id);
                } else {
                    app.set_toast("No server selected.".to_string(), true);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.select_prev();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.select_next();
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                app.should_quit = true;
            }
            _ => {}
        },

        InputMode::PromptStartServer => match code {
            KeyCode::Enter => {
                let addr = app.input_buffer.trim().to_string();
                app.input_mode = InputMode::Normal;
                app.start_server(&addr);
            }
            KeyCode::Esc => {
                app.input_mode = InputMode::Normal;
                app.input_buffer.clear();
            }
            KeyCode::Backspace => {
                app.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                app.input_buffer.push(c);
            }
            _ => {}
        },

        InputMode::PromptBatchLaunch => match code {
            KeyCode::Enter => {
                let input = app.input_buffer.trim().to_string();
                app.input_mode = InputMode::Normal;

                let parts: Vec<&str> = input.split_whitespace().collect();
                let count: usize = parts.first().and_then(|s| s.parse().ok()).unwrap_or(100);
                let rate: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(50);

                app.batch_launch_selected(count, rate);
            }
            KeyCode::Esc => {
                app.input_mode = InputMode::Normal;
                app.input_buffer.clear();
            }
            KeyCode::Backspace => {
                app.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                app.input_buffer.push(c);
            }
            _ => {}
        },
    }
}
