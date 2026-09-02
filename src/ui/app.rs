pub enum Screen {
    Home,
    Dashboard,
    ClientView(usize),
}

pub enum InputMode {
    Selecting,
    Typing,
}

pub struct App {
    pub screen: Screen,
    pub option_selected: usize,
    pub input_mode: InputMode,
}

impl App {
    pub fn new() -> Self {
        App {
            screen: Screen::Home,
            option_selected: 0,
            input_mode: InputMode::Selecting,
        }
    }

    pub fn increment_option_selected(&mut self) {
        if self.option_selected > 0 {
            self.option_selected -= 1;
        }
    }

    pub fn decrement_option_selected(&mut self) {
        if self.option_selected < 1 {
            self.option_selected += 1;
        }
    }
}
