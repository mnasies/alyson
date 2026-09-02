use crate::client::Client;
use std::sync::{Arc, Mutex};

pub struct ClientSummary {
    pub id: usize,
    pub name: String,
}

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
    pub clients: Vec<ClientSummary>,
    pub client_selected: usize,
    pub shared_clients: Arc<Mutex<Vec<Client>>>,
    pub new_client_name: String,
}

impl App {
    pub fn new(shared_clients: Arc<Mutex<Vec<Client>>>) -> Self {
        App {
            screen: Screen::Home,
            option_selected: 0,
            input_mode: InputMode::Selecting,
            clients: Vec::new(),
            client_selected: 0,
            shared_clients,
            new_client_name: String::new(),
        }
    }

    pub fn refresh_clients(&mut self) {
        let clients = self.shared_clients.lock().unwrap();
        self.clients = clients
            .iter()
            .map(|c| ClientSummary {
                id: c.id,
                name: c.username.clone(),
            })
            .collect();
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
