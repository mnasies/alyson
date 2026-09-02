pub enum Screen {
    Login,
    Dashboard,
}

pub struct App {
    pub counter: i32,
    pub screen: Screen,
}

impl App {
    pub fn new() -> Self {
        App {
            counter: 0,
            screen: Screen::Login,
        }
    }

    pub fn increment(&mut self) -> i32 {
        self.counter += 1;
        self.counter
    }

    pub fn decrement(&mut self) -> i32 {
        self.counter -= 1;
        self.counter
    }
}
