use crate::ui::app::App;
use crate::ui::app::Screen;
use crate::ui::dashboard::draw_dashboard;

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
