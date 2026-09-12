use crate::ui::app;
use crate::ui::app::{ActionState, App};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn draw_dashboard(frame: &mut Frame, app: &mut App) {
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

    draw_client_sidebar(frame, chunks[0], app);
    draw_main_pane(frame, chunks[1], app);

    let help_info =
        Paragraph::new("Use ↑↓←→ to navigate,  press 'Enter' to select, press 'q' to quit")
            .block(Block::default().borders(Borders::ALL));
    frame.render_widget(help_info, outer[1]);
}

fn draw_main_pane(frame: &mut Frame, canvas: Rect, app: &mut App) {
    let block_left = Block::default().title(" WIRE CHAT ").borders(Borders::ALL);

    match app.input_mode {
        app::InputMode::Typing => match &app.action_state {
            ActionState::None => {
                let content = format!("New client name:\n\n{}_", app.buf.new_client_name);
                let info = Paragraph::new(content).block(block_left);
                frame.render_widget(info, canvas);
            }
            ActionState::SendMessage => {
                if let app::DashBoardView::ClientView(id, _) = &app.dashboard_view {
                    draw_client_info(frame, canvas, app, *id);
                }
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

fn draw_client_info(frame: &mut Frame, canvas: Rect, app: &mut App, id: usize) {
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

    match &app.dashboard_view {
        app::DashBoardView::ClientView(_, app::CurrentWindow::ActionList) => {
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
        app::DashBoardView::ClientView(_, app::CurrentWindow::Inbox) => {
            let inbox_chunk = Layout::default()
                .direction(Direction::Horizontal)
                .margin(1)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(chunks[1]);

            let mut clients_clone = app.clients.clone();
            clients_clone.retain(|cli| Some(cli.id) != app.current_clients.0);
            let all_clients_list: Vec<ListItem> = clients_clone
                .iter()
                .enumerate()
                .map(|(i, cli)| {
                    let style = match app.selected.inbox_cli_selected {
                        Some(n) => {
                            if i == n {
                                app.current_clients.1 = Some(cli.id);
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

            let outer = Block::default()
                .title(" Send Message ")
                .borders(Borders::ALL);

            let inner_area = outer.inner(inbox_chunk[1]); // area inside the outer border
            frame.render_widget(outer, inbox_chunk[1]); // draw outer box first

            let [messages_area, input_area] =
                Layout::vertical([Constraint::Percentage(80), Constraint::Percentage(20)])
                    .areas(inner_area);
            let messages_list_widget = Paragraph::new("something");

            // messages: no border, just content, sits flush inside outer box
            frame.render_widget(messages_list_widget, messages_area);

            // input: bordered on top only → reads as a "divider line" not a second box
            let style = if app.selected.inbox_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            let input_block = Block::default()
                .borders(Borders::TOP) // just a line, not a full nested box
                .style(style);

            if matches!(app.input_mode, app::InputMode::Typing) {
                let content = format!("{}_", app.buf.msg_to_client);
                let info = Paragraph::new(content).block(input_block);
                frame.render_widget(info, input_area);
            } else {
                let placeholder = Paragraph::new("Press Enter to type a message");
                frame.render_widget(placeholder.block(input_block), input_area);
            }
        }
        _ => {}
    }
}

fn draw_client_sidebar(frame: &mut Frame, canvas: Rect, app: &mut App) {
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
