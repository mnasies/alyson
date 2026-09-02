pub mod app;
pub mod ui;
pub use app::App;
use ui::ui;

use crossterm::event;
use crossterm::event::{Event, KeyCode};
use ratatui::{Terminal, backend::CrosstermBackend};

pub fn run(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> std::io::Result<()> {
    let mut main_app = App::new();
    loop {
        terminal.draw(|frame| ui(frame, &main_app))?; // draw

        if let Event::Key(key) = event::read()? {
            // events
            if key.code == KeyCode::Char('q') {
                break; // respond
            } else if key.code == KeyCode::Char('i') {
                main_app.increment();
            } else if key.code == KeyCode::Char('d') {
                main_app.decrement();
            }
        }
    }
    Ok(())
}
