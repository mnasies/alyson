pub mod app;
pub mod ui;
use crate::client::Client;
pub use app::App;
use ui::ui;

use crossterm::event;
use crossterm::event::{Event, KeyCode};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::sync::{Arc, Mutex};

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    clients: Arc<Mutex<Vec<Client>>>,
) -> std::io::Result<()> {
    let mut main_app = App::new(clients);
    main_app.refresh_clients();

    loop {
        terminal.draw(|frame| ui(frame, &main_app))?; // draw

        if let Event::Key(key) = event::read()? {
            if let KeyCode::Char('q') = key.code {
                break;
            }
            // events
            match main_app.screen {
                app::Screen::Home => match key.code {
                    KeyCode::Up => main_app.increment_option_selected(),
                    KeyCode::Down => main_app.decrement_option_selected(),
                    KeyCode::Enter => {
                        if main_app.option_selected == 1 {
                            break;
                        }
                        main_app.screen = app::Screen::Dashboard;
                    }
                    _ => {}
                },
                app::Screen::Dashboard => match key.code {
                    _ => {}
                },
                _ => {}
            }
        }
    }
    Ok(())
}
