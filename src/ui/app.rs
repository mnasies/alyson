use crate::WireError;
use crate::client::InboxEntry;
use crate::network::NetworkHandle;
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
    ClientView(usize, CurrentWindow),
}

pub enum CurrentWindow {
    None,
    ActionList,
    Inbox,
}

#[derive(Clone)]
pub enum ActionState {
    SendMessage(usize, SendMsgStep),
    JoinChatRoom(usize, JoinRoomStep),
    CreateChatRoom(usize, CreateRoomStep),
    None,
}

#[derive(Clone)]
pub enum SendMsgStep {
    Target,
    Message,
}

#[derive(Clone)]
pub enum JoinRoomStep {
    Target,
    Message,
}

#[derive(Clone)]
pub enum CreateRoomStep {
    Target,
    Message,
}

pub enum Focus {
    Main,
    ClientList,
    ClientOption,
    ActionList,
    InboxCli,
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

pub struct Selection {
    pub client_selected: Option<usize>,
    pub cli_opt_selected: Option<usize>,
    pub action_selected: Option<usize>,
    pub inbox_cli_selected: Option<usize>,
    pub option_selected: usize,
}

impl Selection {
    pub fn new() -> Self {
        Selection {
            client_selected: None,
            cli_opt_selected: None,
            action_selected: None,
            inbox_cli_selected: None,
            option_selected: 0,
        }
    }
}

pub struct App {
    pub screen: Screen,
    pub selected: Selection,
    pub input_mode: InputMode,
    pub dashboard_view: DashBoardView,
    pub clients: Vec<ClientSummary>,
    pub action_state: ActionState,
    pub buf: AppBuf,
    pub focus: Focus,
    pub errors: Vec<(Instant, WireError)>,
    pub inboxes: Vec<InboxEntry>,
}

impl App {
    pub fn new() -> Self {
        App {
            screen: Screen::Home,
            selected: Selection::new(),
            input_mode: InputMode::Selecting,
            dashboard_view: DashBoardView::Idle,
            clients: Vec::new(),
            action_state: ActionState::None,
            buf: AppBuf::new(),
            focus: Focus::None,
            errors: Vec::new(),
            inboxes: Vec::new(),
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

    pub fn verify_client(&mut self, name: String) -> Result<usize, WireError> {
        if name.is_empty() {
            return Err(WireError::InvalidName);
        } else {
            for client in self.clients.iter() {
                if name == client.name {
                    return Ok(client.id);
                }
            }
        }
        Err(WireError::InvalidName)
    }

    pub fn advance_action(&mut self, net_handle: &NetworkHandle) -> Result<(), WireError> {
        let action_state = self.action_state.clone();
        match &action_state {
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
                let target = self.buf.to_client.clone();
                match self.verify_client(target) {
                    Ok(_) => {
                        self.buf.to_client.clear();
                        self.action_state = ActionState::SendMessage(*id, SendMsgStep::Message);
                    }
                    Err(e) => {
                        self.errors.push((Instant::now(), e.clone()));
                        self.input_mode = InputMode::Selecting;
                        self.action_state = ActionState::None;
                        self.buf.to_client.clear();
                    }
                }
            }
            ActionState::SendMessage(id, SendMsgStep::Message) => {
                let msg = self.buf.msg_to_client.trim().to_string();
                if !msg.is_empty() {
                    let _ =
                        net_handle
                            .cmd_tx
                            .try_send(crate::network::NetworkCommand::SendMessage {
                                client_id: *id,
                                msg,
                            });
                }
                self.input_mode = InputMode::Selecting;
                self.buf.msg_to_client.clear();
                self.action_state = ActionState::None;
            }
            _ => {}
        }
        Ok(())
    }
}
