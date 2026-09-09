pub mod app;
mod dashboard;
pub mod ui;

use crate::WireError;
use crate::network::{NetworkEvent, NetworkHandle};
pub use app::{App, DashBoardView, Focus, InputResult};
use std::time::{Duration, Instant};
use ui::{draw_error_toast, ui};

use crossterm::event;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc::Receiver;

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    net_handle: NetworkHandle,
    mut event_rx: Receiver<NetworkEvent>,
) -> Result<(), WireError> {
    let mut main_app = App::new();

    loop {
        // Clean up old errors
        main_app
            .errors
            .retain(|(t, _)| t.elapsed() < Duration::from_secs(5));

        terminal.draw(|frame| {
            ui(frame, &main_app);
            draw_error_toast(frame, &main_app);
        })?;

        // We use a combination of event polling and network event receiving.
        // To avoid blocking the network events, we poll for crossterm events with a short timeout.
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if matches!(main_app.input_mode, app::InputMode::Typing) {
                    handle_input(&mut main_app, key, &net_handle)?;
                    continue;
                }
                if let KeyCode::Char('q') = key.code {
                    break;
                }
                // events
                match main_app.screen {
                    app::Screen::Home => match key.code {
                        KeyCode::Char('e') => {
                            main_app.errors.push((
                                std::time::Instant::now(),
                                WireError::HandshakeFailed("test error".to_string()),
                            ));
                        }
                        KeyCode::Up => {
                            if main_app.selected.option_selected > 0 {
                                main_app.selected.option_selected -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if main_app.selected.option_selected < 1 {
                                main_app.selected.option_selected += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if main_app.selected.option_selected == 1 {
                                break;
                            }
                            if !matches!(main_app.screen, app::Screen::Dashboard) {
                                main_app.screen = app::Screen::Dashboard;
                                main_app.focus = app::Focus::ClientList;
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

        // Check for network events
        while let Ok(event) = event_rx.try_recv() {
            match event {
                NetworkEvent::ClientConnected {
                    id,
                    username,
                    ip,
                    port,
                } => {
                    main_app.clients.push(app::ClientSummary {
                        id,
                        name: username,
                        ip,
                        port,
                    });
                }
                NetworkEvent::ClientDisconnected { id } => {
                    main_app.clients.retain(|c| c.id != id);
                    if let DashBoardView::ClientView(v_id, app::CurrentWindow::ActionList) =
                        main_app.dashboard_view
                    {
                        if v_id == id {
                            main_app.dashboard_view = DashBoardView::Idle;
                        }
                    }
                }
                NetworkEvent::MessageReceived(entry) => {
                    main_app.inboxes.push(entry);
                }
                NetworkEvent::ErrorOccurred(err) => {
                    main_app.errors.push((Instant::now(), err));
                }
            }
        }
    }
    Ok(())
}

fn handle_dashboard_events(key: KeyEvent, main_app: &mut App) {
    match main_app.focus {
        app::Focus::ClientList => match key.code {
            KeyCode::Up => {
                let cli = match main_app.selected.client_selected {
                    Some(n) => n,
                    None => 0,
                };
                if cli > 0 {
                    main_app.selected.client_selected = Some(cli - 1);
                }
            }
            KeyCode::Down => {
                let cli = match main_app.selected.client_selected {
                    Some(n) => n,
                    None => 0,
                };
                let last_index = main_app.clients.len().saturating_sub(1);
                if cli == last_index {
                    main_app.focus = app::Focus::ClientOption;
                    main_app.selected.client_selected = None;
                    main_app.selected.cli_opt_selected = Some(0);
                    return;
                }
                if cli < last_index {
                    main_app.selected.client_selected = Some(cli + 1);
                }
            }
            KeyCode::Enter => match main_app.selected.client_selected {
                Some(n) => {
                    if let Some(client) = main_app.clients.get(n) {
                        main_app.dashboard_view =
                            DashBoardView::ClientView(client.id, app::CurrentWindow::ActionList);
                    }
                }
                _ => {}
            },
            KeyCode::Right => {
                match main_app.dashboard_view {
                    DashBoardView::ClientView(_, app::CurrentWindow::ActionList) => {
                        main_app.focus = app::Focus::ActionList;
                        main_app.selected.action_selected = Some(0);
                        main_app.selected.inbox_cli_selected = None;
                    }
                    DashBoardView::ClientView(_, app::CurrentWindow::Inbox) => {
                        main_app.focus = app::Focus::InboxCli;
                        main_app.selected.action_selected = None;
                        main_app.selected.inbox_cli_selected = Some(0);
                    }
                    _ => {}
                }
                main_app.selected.cli_opt_selected = None;
                main_app.selected.client_selected = None;
            }

            _ => {}
        },
        app::Focus::ClientOption => match key.code {
            KeyCode::Up => {
                if main_app.selected.cli_opt_selected == Some(0) {
                    main_app.selected.cli_opt_selected = None;
                    let last_index = main_app.clients.len().saturating_sub(1);
                    main_app.selected.client_selected = Some(last_index);
                    main_app.focus = app::Focus::ClientList;
                    return;
                }
                let cli_opt = match main_app.selected.cli_opt_selected {
                    Some(n) => n,
                    None => 0,
                };
                if cli_opt > 0 {
                    main_app.selected.cli_opt_selected = Some(cli_opt - 1);
                }
            }
            KeyCode::Down => {
                let cli_opt = match main_app.selected.cli_opt_selected {
                    Some(n) => n,
                    None => 0,
                };
                if cli_opt < 1 {
                    main_app.selected.cli_opt_selected = Some(cli_opt + 1);
                }
            }
            KeyCode::Enter => {
                if let Some(0) = main_app.selected.cli_opt_selected {
                    main_app.input_mode = app::InputMode::Typing;
                    main_app.buf.new_client_name.clear();
                }
            }
            KeyCode::Right => {
                match main_app.dashboard_view {
                    DashBoardView::ClientView(_, app::CurrentWindow::ActionList) => {
                        main_app.focus = app::Focus::ActionList;
                        main_app.selected.action_selected = Some(0);
                        main_app.selected.inbox_cli_selected = None;
                    }
                    DashBoardView::ClientView(_, app::CurrentWindow::Inbox) => {
                        main_app.focus = app::Focus::InboxCli;
                        main_app.selected.action_selected = None;
                        main_app.selected.inbox_cli_selected = Some(0);
                    }
                    _ => {}
                }
                main_app.selected.cli_opt_selected = None;
                main_app.selected.client_selected = None;
            }
            _ => {}
        },
        app::Focus::ActionList => match key.code {
            KeyCode::Up => {
                let action = match main_app.selected.action_selected {
                    Some(n) => n,
                    None => 0,
                };
                if action > 0 {
                    main_app.selected.action_selected = Some(action - 1);
                }
            }
            KeyCode::Down => {
                let action = match main_app.selected.action_selected {
                    Some(n) => n,
                    None => 0,
                };
                if action < 2 {
                    main_app.selected.action_selected = Some(action + 1);
                }
            }
            KeyCode::Left => {
                main_app.focus = app::Focus::ClientList;
                main_app.selected.action_selected = None;
                main_app.selected.client_selected = Some(0);
            }
            KeyCode::Enter => match main_app.selected.action_selected {
                Some(0) => {
                    // main_app.input_mode = app::InputMode::Typing;
                    if let DashBoardView::ClientView(id, app::CurrentWindow::ActionList) =
                        main_app.dashboard_view
                    {
                        main_app.action_state =
                            app::ActionState::SendMessage(id, app::SendMsgStep::Target);
                        main_app.selected.action_selected = None;
                        main_app.selected.inbox_cli_selected = Some(0);
                        main_app.focus = app::Focus::InboxCli;
                        main_app.dashboard_view =
                            DashBoardView::ClientView(id, app::CurrentWindow::Inbox);
                    }
                }
                _ => {}
            },
            _ => {}
        },
        app::Focus::InboxCli => match key.code {
            KeyCode::Up => {
                let cli = match main_app.selected.inbox_cli_selected {
                    Some(n) => n,
                    None => 0,
                };
                if cli > 0 {
                    main_app.selected.inbox_cli_selected = Some(cli - 1);
                }
            }
            KeyCode::Down => {
                let cli = match main_app.selected.inbox_cli_selected {
                    Some(n) => n,
                    None => 0,
                };
                let last_index = main_app.clients.len().saturating_sub(1);
                if cli < last_index {
                    main_app.selected.inbox_cli_selected = Some(cli + 1);
                }
            }
            KeyCode::Left => {
                main_app.focus = app::Focus::ClientList;
                main_app.selected.action_selected = None;
                main_app.selected.client_selected = Some(0);
                main_app.selected.inbox_cli_selected = None;
            }
            KeyCode::Enter => {}
            _ => {}
        },
        app::Focus::None => {}
        _ => {}
    }
}

fn handle_input(
    main_app: &mut App,
    key: KeyEvent,
    net_handle: &NetworkHandle,
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
