use crate::ui::app;
use crate::ui::app::{ActionState, ActionState::SendMessage, App, SendMsgStep};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn draw_dashboard(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // main area
            Constraint::Length(3), // help text
        ])
        .split(area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // info text area
            Constraint::Percentage(75), // options list, takes remaining space
        ])
        .split(outer[0]);

    draw_client_sidebar(frame, chunks[0], &app);
    draw_main_pane(frame, chunks[1], &app);

    let help_info =
        Paragraph::new("Use ↑↓←→ to navigate,  press 'Enter' to select, press 'q' to quit")
            .block(Block::default().borders(Borders::ALL));
    frame.render_widget(help_info, outer[1]);
}

fn draw_main_pane(frame: &mut Frame, canvas: Rect, app: &App) {
    let block_left = Block::default().title(" WIRE CHAT ").borders(Borders::ALL);

    match app.input_mode {
        app::InputMode::Typing => match &app.action_state {
            SendMessage(n, _s) => {
                draw_client_info(frame, canvas, app, *n);
            }
            ActionState::None => {
                let content = format!("New client name:\n\n{}_", app.buf.new_client_name);
                let info = Paragraph::new(content).block(block_left);
                frame.render_widget(info, canvas);
            }
            _ => {}
        },
        app::InputMode::Selecting => match app.dashboard_view {
            app::DashBoardView::Idle => {
                let content = "Select a client, or choose an action from the sidebar.".to_string();
                let info = Paragraph::new(content).block(block_left);
                frame.render_widget(info, canvas);
            }
            app::DashBoardView::ClientView(id, _) => {
                draw_client_info(frame, canvas, app, id);
                return;
            }
        },
    };
}

fn draw_client_info(frame: &mut Frame, canvas: Rect, app: &App, id: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20), // info text area
            Constraint::Percentage(80), // options list, takes remaining space
        ])
        .split(canvas);
    let client_name = match app.clients.iter().find(|c| c.id == id) {
        Some(c) => c.name.clone(),
        None => "Unknown client".to_string(),
    };
    let upper_block = Block::default()
        .title(client_name.clone())
        .borders(Borders::ALL);

    let ip = match app.clients.iter().find(|c| c.id == id) {
        Some(c) => c.ip.clone(),
        None => "Unknown IP".to_string(),
    };
    let port = match app.clients.iter().find(|c| c.id == id) {
        Some(c) => c.port,
        None => 0,
    };
    let info = format!(
        "Client: {}    IP Address: {}    Port Number: {}",
        client_name.clone(),
        ip,
        port
    );
    let info_block = Paragraph::new(info).block(upper_block);
    frame.render_widget(info_block, chunks[0]);

    match &app.action_state {
        ActionState::None => {
            let lower_block = Block::default().title(" Actions ").borders(Borders::ALL);

            let actions = vec![
                String::from("Inboxes"),
                String::from("Chat Rooms"),
                String::from("Exit"),
            ];
            let action_list: Vec<ListItem> = actions
                .iter()
                .enumerate()
                .map(|(i, act)| {
                    let style = match app.selected.action_selected {
                        Some(n) => {
                            if i == n {
                                Style::default().add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default()
                            }
                        }
                        None => Style::default(),
                    };
                    let line = Line::from(act.clone()).alignment(Alignment::Left);
                    ListItem::new(line).style(style)
                })
                .collect();
            let actions_list = List::new(action_list).block(lower_block);
            frame.render_widget(actions_list, chunks[1]);
        }
        ActionState::SendMessage(_id, _step) => {
            let lower_block = Block::default()
                .title(" Send Message ")
                .borders(Borders::ALL);

            let inbox_chunk = Layout::default()
                .direction(Direction::Horizontal)
                .margin(1)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(chunks[1]);

            let all_clients_list: Vec<ListItem> = app
                .clients
                .iter()
                .enumerate()
                .map(|(i, cli)| {
                    let style = match app.selected.inbox_cli_selected {
                        Some(n) => {
                            if i == n {
                                Style::default().add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default()
                            }
                        }
                        None => Style::default(),
                    };
                    let line = Line::from(cli.name.clone()).alignment(Alignment::Left);
                    ListItem::new(line).style(style)
                })
                .collect();
            let list = List::new(all_clients_list).block(Block::default().borders(Borders::all()));

            frame.render_widget(list, inbox_chunk[0]);

            frame.render_widget(lower_block, inbox_chunk[1]);
        }
        _ => {}
    }
}

fn draw_client_sidebar(frame: &mut Frame, canvas: Rect, app: &App) {
    let block_right = Block::default()
        .title(" ALL CLIENTS ")
        .borders(Borders::ALL);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(canvas);

    let clients = &app.clients;
    let client_list: Vec<ListItem> = clients
        .iter()
        .enumerate()
        .map(|(i, cli)| {
            let style = match app.selected.client_selected {
                Some(n) => {
                    if i == n {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    }
                }
                None => Style::default(),
            };
            ListItem::new(cli.name.clone()).style(style)
        })
        .collect();
    let list = List::new(client_list).block(block_right);

    frame.render_widget(list, chunks[0]);

    let options = vec![
        String::from("New Client"),
        String::from("Create New Chat Room"),
    ];
    let cli_opt_list: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = match app.selected.cli_opt_selected {
                Some(n) => {
                    if i == n {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    }
                }
                None => Style::default(),
            };
            ListItem::new(opt.clone()).style(style)
        })
        .collect();
    let list = List::new(cli_opt_list).block(Block::default().borders(Borders::ALL));
    frame.render_widget(list, chunks[1]);
}
