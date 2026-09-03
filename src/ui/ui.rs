use crate::ui::app;
use crate::ui::app::App;
use crate::ui::app::Screen;

use ratatui::layout::Rect;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn ui(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Home => draw_homepage(frame, app),
        Screen::Dashboard => draw_dashboard(frame, app),
        Screen::ClientView(_n) => {}
    }
}

fn draw_homepage(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let block = Block::default().title(" WIRE-CHAT ").borders(Borders::ALL);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // info text area
            Constraint::Min(0),    // options list, takes remaining space
        ])
        .split(area);

    let info = Paragraph::new("Wire\n\nA TCP networking sandbox for learning.\n").block(block);
    frame.render_widget(info, chunks[0]);
    let options = vec!["Start", "Quit"];
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == app.option_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(*opt).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Choose"));
    frame.render_widget(list, chunks[1]);
}

fn draw_dashboard(frame: &mut Frame, app: &App) {
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

    let content = match app.input_mode {
        app::InputMode::Typing => {
            format!("New client name:\n\n{}_", app.new_client_name)
        }
        app::InputMode::Selecting => {
            "Select a client, or choose an action from the sidebar.".to_string()
        }
    };

    let info = Paragraph::new(content).block(block_left);
    frame.render_widget(info, canvas);
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

pub fn draw_error_toast(frame: &mut Frame, app: &App) {
    let errors = app.errors.lock().unwrap();
    if let Some(latest) = errors.last() {
        let area = frame.area();
        let toast_area = Rect {
            x: area.width.saturating_sub(40),
            y: 0,
            width: 40.min(area.width),
            height: 3,
        };
        let block = Block::default()
            .title(" Error ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));
        let text = Paragraph::new(latest.1.to_string())
            .style(Style::default().fg(Color::Red))
            .block(block);
        frame.render_widget(text, toast_area);
    }
}
