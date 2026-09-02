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

pub enum Focus {
    Main,
    ClientList,
    ClientOption,
    None,
}

pub struct App {
    pub screen: Screen,
    pub option_selected: usize,
    pub input_mode: InputMode,
    pub clients: Vec<ClientSummary>,
    pub client_selected: usize,
    pub cli_opt_selected: Option<usize>,
    pub shared_clients: Arc<Mutex<Vec<Client>>>,
    pub new_client_name: String,
    pub focus: Focus,
}

impl App {
    pub fn new(shared_clients: Arc<Mutex<Vec<Client>>>) -> Self {
        App {
            screen: Screen::Home,
            option_selected: 0,
            input_mode: InputMode::Selecting,
            clients: Vec::new(),
            client_selected: 0,
            cli_opt_selected: None,
            shared_clients,
            new_client_name: String::new(),
            focus: Focus::None,
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
}
