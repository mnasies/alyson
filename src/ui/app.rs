use crate::WireError;
use crate::client::Client;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct ClientSummary {
    pub id: usize,
    pub name: String,
    pub ip: String,
    pub port: u16,
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

pub enum DashBoardView {
    Idle,
    ClientView(usize),
}

pub enum Focus {
    Main,
    ClientList,
    ClientOption,
    ActionList,
    None,
}

pub struct App {
    pub screen: Screen,
    pub option_selected: usize,
    pub input_mode: InputMode,
    pub dashboard_view: DashBoardView,
    pub clients: Vec<ClientSummary>,
    pub client_selected: Option<usize>,
    pub cli_opt_selected: Option<usize>,
    pub action_selected: Option<usize>,
    pub shared_clients: Arc<Mutex<Vec<Client>>>,
    pub new_client_name: String,
    pub focus: Focus,
    pub errors: Arc<Mutex<Vec<(Instant, WireError)>>>,
}

impl App {
    pub fn new(
        shared_clients: Arc<Mutex<Vec<Client>>>,
        errors: Arc<Mutex<Vec<(Instant, WireError)>>>,
    ) -> Self {
        App {
            screen: Screen::Home,
            option_selected: 0,
            input_mode: InputMode::Selecting,
            dashboard_view: DashBoardView::Idle,
            clients: Vec::new(),
            client_selected: Some(0),
            cli_opt_selected: None,
            action_selected: None,
            shared_clients,
            new_client_name: String::new(),
            focus: Focus::None,
            errors,
        }
    }

    pub fn refresh_clients(&mut self) {
        let clients = self.shared_clients.lock().unwrap();
        self.clients = clients
            .iter()
            .map(|c| ClientSummary {
                id: c.id,
                name: c.username.clone(),
                ip: c.ip.clone(),
                port: c.port,
            })
            .collect();
    }
}
