pub mod app;
pub mod ui;

use crate::WireError;
use crate::client::Client;
use crate::network::NetworkHandle;
pub use app::App;
pub use app::Focus;
use crossterm::event::KeyEvent;
use std::time::Duration;
use std::time::Instant;
use ui::draw_error_toast;
use ui::ui;

use crossterm::event;
use crossterm::event::{Event, KeyCode};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::sync::{Arc, Mutex};

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    clients: Arc<Mutex<Vec<Client>>>,
    mut net_handle: NetworkHandle,
    errors: Arc<Mutex<Vec<(Instant, WireError)>>>,
) -> Result<(), WireError> {
    let mut main_app = App::new(clients, errors);
    // eprintln!("errors len: {}", main_app.errors.lock().unwrap().len());

    loop {
        main_app.refresh_clients();
        main_app
            .errors
            .lock()
            .unwrap()
            .retain(|(t, _)| t.elapsed() < Duration::from_secs(5));

        terminal.draw(|frame| {
            ui(frame, &main_app);
            draw_error_toast(frame, &main_app);
        })?; // draw

        if !event::poll(std::time::Duration::from_millis(16))? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            if matches!(main_app.input_mode, app::InputMode::Typing) {
                match key.code {
                    KeyCode::Char(c) => main_app.new_client_name.push(c),
                    KeyCode::Backspace => {
                        main_app.new_client_name.pop();
                    }
                    KeyCode::Enter => {
                        let name = main_app.new_client_name.trim().to_string();
                        main_app.input_mode = app::InputMode::Selecting;
                        if !name.is_empty() {
                            // actually create the client — network call, next
                            net_handle.spawn_client(name)?;
                        }
                        main_app.new_client_name.clear();
                    }
                    KeyCode::Esc => {
                        main_app.input_mode = app::InputMode::Selecting;
                        main_app.new_client_name.clear();
                    }
                    _ => {}
                }
                continue;
            }
            if let KeyCode::Char('q') = key.code {
                break;
            }
            // events
            match main_app.screen {
                app::Screen::Home => match key.code {
                    KeyCode::Char('e') => {
                        main_app.errors.lock().unwrap().push((
                            std::time::Instant::now(),
                            WireError::HandshakeFailed("test error".to_string()),
                        ));
                    }
                    KeyCode::Up => {
                        if main_app.option_selected > 0 {
                            main_app.option_selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if main_app.option_selected < 1 {
                            main_app.option_selected += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if main_app.option_selected == 1 {
                            break;
                        }
                        if !matches!(main_app.screen, app::Screen::Dashboard) {
                            main_app.screen = app::Screen::Dashboard;
                            main_app.focus = app::Focus::ClientList; // set once, on transition
                        }
                    }
                    _ => {}
                },
                app::Screen::Dashboard => {
                    handle_dashboard_events(key, &mut main_app);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn handle_dashboard_events(key: KeyEvent, main_app: &mut App) {
    match main_app.focus {
        app::Focus::ClientList => match key.code {
            KeyCode::Up => {
                let cli = match main_app.client_selected {
                    Some(n) => n,
                    None => 0,
                };
                if cli > 0 {
                    main_app.client_selected = Some(cli - 1);
                }
            }
            KeyCode::Down => {
                let cli = match main_app.client_selected {
                    Some(n) => n,
                    None => 0,
                };
                let last_index = main_app.clients.len().saturating_sub(1);
                if cli == last_index {
                    main_app.focus = app::Focus::ClientOption;
                    main_app.client_selected = None;
                    main_app.cli_opt_selected = Some(0);
                    return;
                }
                if cli < last_index {
                    main_app.client_selected = Some(cli + 1);
                }
            }

            _ => {}
        },
        app::Focus::ClientOption => match key.code {
            KeyCode::Up => {
                if main_app.cli_opt_selected == Some(0) {
                    main_app.cli_opt_selected = None;
                    let last_index = main_app.clients.len().saturating_sub(1);
                    main_app.client_selected = Some(last_index);
                    main_app.focus = app::Focus::ClientList;
                    return;
                }
                let cli_opt = match main_app.cli_opt_selected {
                    Some(n) => n,
                    None => 0,
                };
                if cli_opt > 0 {
                    main_app.cli_opt_selected = Some(cli_opt - 1);
                }
            }
            KeyCode::Down => {
                let cli_opt = match main_app.cli_opt_selected {
                    Some(n) => n,
                    None => 0,
                };
                if cli_opt < 1 {
                    main_app.cli_opt_selected = Some(cli_opt + 1);
                }
            }
            KeyCode::Enter => {
                if let Some(0) = main_app.cli_opt_selected {
                    main_app.input_mode = app::InputMode::Typing;
                    main_app.new_client_name.clear();
                }
            }
            _ => {}
        },
        app::Focus::None => {}
        _ => {}
    }
}
