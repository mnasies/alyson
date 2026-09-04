use crate::ui::app;
use crate::ui::app::App;

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
        Paragraph::new("Use ↑↓ to navigate,  press 'Enter' to select,  'n' - new client, 'r' - new room, 'q' - quit")
            .block(Block::default().borders(Borders::ALL));
    frame.render_widget(help_info, outer[1]);
}

fn draw_main_pane(frame: &mut Frame, canvas: Rect, app: &App) {
    let block_left = Block::default().title(" WIRE CHAT ").borders(Borders::ALL);

    let content;

    match app.input_mode {
        app::InputMode::Typing => {
            content = format!("New client name:\n\n{}_", app.new_client_name);
        }
        app::InputMode::Selecting => match app.dashboard_view {
            app::DashBoardView::Idle => {
                content = "Select a client, or choose an action from the sidebar.".to_string();
            }
            app::DashBoardView::ClientView(id) => {
                draw_client_info(frame, canvas, app, id);
                return;
            }
        },
    };

    let info = Paragraph::new(content).block(block_left);
    frame.render_widget(info, canvas);
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

    let lower_block = Block::default().title(" Actions ").borders(Borders::ALL);

    let actions = vec![
        String::from("Send Message"),
        String::from("Create Chat Room"),
        String::from("Join Chat Room"),
        String::from("See Joined Chat Rooms"),
    ];
    let action_list: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, act)| {
            let style = match app.action_selected {
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
            let style = match app.client_selected {
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
            let style = match app.cli_opt_selected {
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
