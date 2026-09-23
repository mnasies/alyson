use crate::WireError;
use crate::network::NetworkHandle;
use crate::types::{InboxEntry, Room};
use std::time::Instant;

#[derive(Clone)]
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
    Room,
}

#[derive(Clone)]
pub enum ActionState {
    SendMessage,
    JoinChatRoom,
    CreateChatRoom,
    SendMessageToRoom,
    None,
}

pub enum Focus {
    Main,
    ClientList,
    ClientOption,
    ActionList,
    InboxCli,
    InboxWindow,
    RoomboxWindow,
    RoomInterface,
    None,
}

pub enum IdentityMode {
    Default,
    Fixed(usize),
}

pub struct AppBuf {
    pub new_client_name: String,
    pub to_client: String,
    pub msg_to_client: String,
    pub new_room_name: String,
}

impl AppBuf {
    pub fn new() -> Self {
        AppBuf {
            new_client_name: String::new(),
            to_client: String::new(),
            msg_to_client: String::new(),
            new_room_name: String::new(),
        }
    }
}

pub struct Selection {
    pub client_selected: Option<usize>,
    pub cli_opt_selected: Option<usize>,
    pub action_selected: Option<usize>,
    pub inbox_cli_selected: Option<usize>,
    pub room_list_selected: Option<usize>,
    pub inbox_selected: bool,
    pub roombox_selected: bool,
    pub option_selected: usize,
}

impl Selection {
    pub fn new() -> Self {
        Selection {
            client_selected: None,
            cli_opt_selected: None,
            action_selected: None,
            inbox_cli_selected: None,
            room_list_selected: None,
            inbox_selected: false,
            roombox_selected: false,
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
    pub current_clients: (Option<usize>, Option<usize>), // (current sender client, current receiver client)
    pub current_room: Option<usize>,
    pub messages_scroll: usize, // lines scrolled up from the bottom; 0 = pinned to newest
    pub rooms: Vec<Room>,
    pub identity_mode: IdentityMode,
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
            current_clients: (None, None),
            current_room: None,
            messages_scroll: 0,
            rooms: Vec::new(),
            identity_mode: IdentityMode::Default,
        }
    }

    pub fn current_input_mut(&mut self) -> Option<&mut String> {
        match self.action_state {
            ActionState::None => Some(&mut self.buf.new_client_name),
            ActionState::SendMessage | ActionState::SendMessageToRoom => {
                Some(&mut self.buf.msg_to_client)
            }
            ActionState::CreateChatRoom => Some(&mut self.buf.new_room_name),
            _ => None,
        }
    }

    pub fn verify_current_room_member(&mut self) -> bool {
        if let Some(room_id) = self.current_room {
            if let Some(room) = self.rooms.iter().find(|room| room.id as usize == room_id) {
                if let Some(cli_id) = self.current_clients.0 {
                    if room.is_member(cli_id as u64) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn verify_client(&mut self, name: String) -> Result<usize, WireError> {
        if name.is_empty() {
            return Err(WireError::InvalidName);
        } else {
            for client in self.clients.iter() {
                if name == client.name {
                    return Ok(client.id as usize);
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
            ActionState::CreateChatRoom => {
                let name = self.buf.new_room_name.trim().to_string();
                self.input_mode = InputMode::Selecting;
                if !name.is_empty() {
                    // actually create the client — network call, next
                    net_handle.create_room(name)?;
                }
                self.buf.new_room_name.clear();
            }
            ActionState::SendMessage | ActionState::SendMessageToRoom => {
                let msg = self.buf.msg_to_client.trim().to_string();
                if !msg.is_empty() {
                    let sender = match self.current_clients.0 {
                        Some(id) => id,
                        None => {
                            self.input_mode = InputMode::Selecting;
                            self.errors
                                .push((std::time::Instant::now(), WireError::SenderNotFound));
                            return Ok(());
                        }
                    };
                    let receiver;
                    let cli_or_room;
                    if matches!(self.action_state, ActionState::SendMessage) {
                        receiver = match self.current_clients.1 {
                            Some(id) => id,
                            None => {
                                self.errors
                                    .push((std::time::Instant::now(), WireError::ReceiverNotFound));
                                self.input_mode = InputMode::Selecting;
                                return Ok(());
                            }
                        };
                        cli_or_room = true;
                    } else {
                        receiver = match self.current_room {
                            Some(room) => room,
                            None => {
                                self.errors
                                    .push((std::time::Instant::now(), WireError::ReceiverNotFound));
                                self.input_mode = InputMode::Selecting;
                                return Ok(());
                            }
                        };
                        cli_or_room = false;
                    }
                    let entry = InboxEntry::new(
                        std::time::SystemTime::now(),
                        msg,
                        sender as u64,
                        receiver as u64,
                        cli_or_room,
                    );
                    if let Err(e) = net_handle.send_message(entry) {
                        self.errors.push((std::time::Instant::now(), e));
                    }
                    self.messages_scroll = 0;
                }
                self.input_mode = InputMode::Selecting;
                self.buf.msg_to_client.clear();
            }
            _ => {}
        }
        Ok(())
    }
}
