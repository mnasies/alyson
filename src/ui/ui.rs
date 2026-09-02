use crate::ui::app::App;
use crate::ui::app::Screen;

use ratatui::{
    Frame,
    widgets::{Block, Borders, Paragraph},
};

pub fn ui(frame: &mut Frame, app: &App) {
    let area = frame.area(); // whole terminal size, as a Rect

    let block = Block::default().title("Wire").borders(Borders::ALL);

    match app.screen {
        Screen::Login => {}
        Screen::Dashboard => {}
    }

    let text = format!(
        "Counter: {}\n\nPress 'i' to increment, 'd' to decrement, 'q' to quit",
        app.counter
    );
    let paragraph = Paragraph::new(text).block(block);

    frame.render_widget(paragraph, area);
}
