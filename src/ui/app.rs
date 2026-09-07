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
    pub buf: AppBuf,
    pub focus: Focus,
    pub errors: Vec<(Instant, WireError)>,
    pub inboxes: Vec<InboxEntry>,
}

impl App {
    pub fn new() -> Self {
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

    pub fn advance_action(&mut self, net_handle: &NetworkHandle) -> Result<(), WireError> {
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
            ActionState::SendMessage(id, SendMsgStep::Message) => {
                let msg = self.buf.msg_to_client.trim().to_string();
                if !msg.is_empty() {
                    let _ = net_handle.cmd_tx.blocking_send(
                        crate::network::NetworkCommand::SendMessage {
                            client_id: *id,
                            msg,
                        },
                    );
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
