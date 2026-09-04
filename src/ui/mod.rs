pub mod app;
mod dashboard;
pub mod ui;

use crate::WireError;
use crate::client::Client;
use crate::network::NetworkHandle;
pub use app::{App, DashBoardView, Focus, InputResult};
use std::time::{Duration, Instant};
use ui::{draw_error_toast, ui};

use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyEvent};
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
                handle_input(&mut main_app, key, &mut net_handle)?;
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
            KeyCode::Enter => match main_app.client_selected {
                Some(n) => {
                    if let Some(client) = main_app.clients.get(n) {
                        main_app.dashboard_view = DashBoardView::ClientView(client.id);
                    }
                }
                _ => {}
            },
            KeyCode::Right => {
                main_app.focus = app::Focus::ActionList;
                main_app.action_selected = Some(0);
                main_app.cli_opt_selected = None;
                main_app.client_selected = None;
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
                    main_app.buf.new_client_name.clear();
                }
            }
            KeyCode::Right => {
                main_app.focus = app::Focus::ActionList;
                main_app.action_selected = Some(0);
                main_app.cli_opt_selected = None;
                main_app.client_selected = None;
            }
            _ => {}
        },
        app::Focus::ActionList => match key.code {
            KeyCode::Up => {
                let action = match main_app.action_selected {
                    Some(n) => n,
                    None => 0,
                };
                if action > 0 {
                    main_app.action_selected = Some(action - 1);
                }
            }
            KeyCode::Down => {
                let action = match main_app.action_selected {
                    Some(n) => n,
                    None => 0,
                };
                if action < 3 {
                    main_app.action_selected = Some(action + 1);
                }
            }
            KeyCode::Left => {
                main_app.focus = app::Focus::ClientList;
                main_app.action_selected = None;
                main_app.client_selected = Some(0);
            }
            KeyCode::Enter => match main_app.action_selected {
                Some(0) => {
                    main_app.input_mode = app::InputMode::Typing;
                    if let DashBoardView::ClientView(id) = main_app.dashboard_view {
                        main_app.action_state =
                            app::ActionState::SendMessage(id, app::SendMsgStep::Target);
                        main_app.input_mode = app::InputMode::Typing;
                    }
                }
                _ => {}
            },
            _ => {}
        },
        app::Focus::None => {}
        _ => {}
    }
}

fn handle_input(
    main_app: &mut App,
    key: KeyEvent,
    net_handle: &mut NetworkHandle,
) -> Result<(), WireError> {
    let Some(buf) = main_app.current_input_mut() else {
        return Ok(());
    };
    let result = handle_basic_inputting(buf, key);
    match result {
        InputResult::Continue => {}
        InputResult::Cancelled => {
            main_app.action_state = app::ActionState::None;
            main_app.input_mode = app::InputMode::Selecting;
        }
        InputResult::Submitted => main_app.advance_action(net_handle)?,
    }
    Ok(())
}

fn handle_basic_inputting(buf: &mut String, key: KeyEvent) -> InputResult {
    match key.code {
        KeyCode::Char(c) => {
            buf.push(c);
            InputResult::Continue
        }
        KeyCode::Backspace => {
            buf.pop();
            InputResult::Continue
        }
        KeyCode::Enter => InputResult::Submitted,
        KeyCode::Esc => {
            buf.clear();
            InputResult::Cancelled
        }
        _ => InputResult::Continue,
    }
}
