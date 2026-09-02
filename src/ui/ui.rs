use crate::ui::app::App;
use crate::ui::app::ClientSummary;
use crate::ui::app::Screen;

use ratatui::layout::Rect;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
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
    let block_left = Block::default()
        .title(" ALL CLIENTS ")
        .borders(Borders::ALL);

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

    let client_info =
        Paragraph::new("Wire\n\nA TCP networking sandbox for learning.\n").block(block_left);
    frame.render_widget(client_info, chunks[1]);
    let help_info =
        Paragraph::new("Use ↑↓ to navigate,  press 'Enter' to select,  'n' - new client, 'r' - new room, 'q' - quit")
            .block(Block::default().borders(Borders::ALL));
    frame.render_widget(help_info, outer[1]);
}

fn draw_client_sidebar(frame: &mut Frame, canvas: Rect, app: &App) {
    let block_right = Block::default().title(" WIRE-CHAT ").borders(Borders::ALL);
    let clients = &app.clients;
    let client_list: Vec<ListItem> = clients
        .iter()
        .enumerate()
        .map(|(i, cli)| {
            let style = if i == app.client_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            ListItem::new(cli.name.clone()).style(style)
        })
        .collect();
    let list = List::new(client_list).block(block_right);

    frame.render_widget(list, canvas);
}
