use crate::WireError;
use crate::client::{Client, InboxEntry};
use crate::network::NetworkHandle;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct ClientSummary {
    pub id: usize,
    pub name: String,
    pub ip: String,
    pub port: u16,
}

pub enum InputResult {
    Continue,
    Cancelled,
    Submitted,
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

pub enum ActionState {
    SendMessage(usize, SendMsgStep),
    JoinChatRoom(usize, JoinRoomStep),
    CreateChatRoom(usize, CreateRoomStep),
    None,
}

pub enum SendMsgStep {
    Target,
    Message,
}

pub enum JoinRoomStep {
    Target,
    Message,
}

pub enum CreateRoomStep {
    Target,
    Message,
}

pub enum Focus {
    Main,
    ClientList,
    ClientOption,
    ActionList,
    None,
}

pub struct AppBuf {
    pub new_client_name: String,
    pub to_client: String,
    pub msg_to_client: String,
}

impl AppBuf {
    pub fn new() -> Self {
        AppBuf {
            new_client_name: String::new(),
            to_client: String::new(),
            msg_to_client: String::new(),
        }
    }
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
    pub action_state: ActionState,
    pub shared_clients: Arc<Mutex<Vec<Client>>>,
    pub buf: AppBuf,
    pub focus: Focus,
    pub errors: Arc<Mutex<Vec<(Instant, WireError)>>>,
    pub outgoing: Arc<Mutex<Vec<InboxEntry>>>,
    pub inboxes: Arc<Mutex<Vec<InboxEntry>>>,
}

impl App {
    pub fn new(
        shared_clients: Arc<Mutex<Vec<Client>>>,
        errors: Arc<Mutex<Vec<(Instant, WireError)>>>,
        outgoing: Arc<Mutex<Vec<InboxEntry>>>,
        inboxes: Arc<Mutex<Vec<InboxEntry>>>,
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
            action_state: ActionState::None,
            shared_clients,
            buf: AppBuf::new(),
            focus: Focus::None,
            errors,
            outgoing,
            inboxes,
        }
    }

    pub fn current_input_mut(&mut self) -> Option<&mut String> {
        match self.action_state {
            ActionState::None => Some(&mut self.buf.new_client_name),
            ActionState::SendMessage(_, SendMsgStep::Target) => Some(&mut self.buf.to_client),
            ActionState::SendMessage(_, SendMsgStep::Message) => Some(&mut self.buf.msg_to_client),
            _ => None,
        }
    }

    pub fn advance_action(&mut self, net_handle: &mut NetworkHandle) -> Result<(), WireError> {
        match &self.action_state {
            ActionState::None => {
                let name = self.buf.new_client_name.trim().to_string();
                self.input_mode = InputMode::Selecting;
                if !name.is_empty() {
                    // actually create the client — network call, next
                    net_handle.spawn_client(name)?;
                }
                self.buf.new_client_name.clear();
            }
            ActionState::SendMessage(id, SendMsgStep::Target) => {
                self.input_mode = InputMode::Typing;
                self.buf.to_client.clear();
                self.action_state = ActionState::SendMessage(*id, SendMsgStep::Message);
            }
            ActionState::SendMessage(_, SendMsgStep::Message) => {
                self.input_mode = InputMode::Selecting;
                self.buf.msg_to_client.clear();
                self.action_state = ActionState::None;
            }
            _ => {}
        }
        Ok(())
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
