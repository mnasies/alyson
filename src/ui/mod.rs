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
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Up => main_app.increment_option_selected(),
                KeyCode::Down => main_app.decrement_option_selected(),
                KeyCode::Enter => {
                    if main_app.option_selected == 1 {
                        break;
                    }
                    main_app.screen = app::Screen::Dashboard;
                }
                _ => {}
            }
        }
    }
    Ok(())
}
