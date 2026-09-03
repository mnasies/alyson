pub mod app;
pub mod ui;

use crate::client::Client;
use crate::network::NetworkHandle;
pub use app::App;
pub use app::Focus;
use crossterm::event::KeyEvent;
use ui::ui;

use crossterm::event;
use crossterm::event::{Event, KeyCode};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::sync::{Arc, Mutex};

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    clients: Arc<Mutex<Vec<Client>>>,
    mut net_handle: NetworkHandle,
) -> std::io::Result<()> {
    let mut main_app = App::new(clients);

    loop {
        main_app.refresh_clients();
        terminal.draw(|frame| ui(frame, &main_app))?; // draw

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
                            net_handle.spawn_client(name);
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
                if main_app.client_selected > 0 {
                    main_app.client_selected -= 1;
                }
            }
            KeyCode::Down => {
                let last_index = main_app.clients.len().saturating_sub(1);
                if main_app.client_selected == last_index {
                    main_app.focus = app::Focus::ClientOption;
                    main_app.cli_opt_selected = Some(0);
                }
                if main_app.client_selected < last_index {
                    main_app.client_selected += 1;
                }
            }

            _ => {}
        },
        app::Focus::ClientOption => match key.code {
            KeyCode::Up => {
                if main_app.cli_opt_selected == Some(0) {
                    main_app.cli_opt_selected = None;
                    main_app.focus = app::Focus::ClientList;
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
