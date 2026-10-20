use crate::AlysonError;
use crate::network::NetworkHandle;
use crate::types::{ClientInfo, InboxEntry, Room};
use std::time::Instant;

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
    ClientView(u64, CurrentWindow),
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
    CreateChatRoom(u64),
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
    Fixed(Option<u64>),
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
    pub clients: Vec<ClientInfo>,
    pub action_state: ActionState,
    pub buf: AppBuf,
    pub focus: Focus,
    pub errors: Vec<(Instant, AlysonError)>,
    pub inboxes: Vec<InboxEntry>,
    pub current_clients: (Option<u64>, Option<u64>), // (current sender client, current receiver client)
    pub current_room: Option<u64>,
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
        match self.focus {
            Focus::ClientOption => match self.selected.cli_opt_selected {
                Some(0) => Some(&mut self.buf.new_room_name),
                Some(1) => Some(&mut self.buf.new_client_name),
                _ => None,
            },
            Focus::InboxWindow => Some(&mut self.buf.msg_to_client),
            Focus::RoomboxWindow => Some(&mut self.buf.msg_to_client),
            _ => None,
        }
    }

    pub fn verify_current_room_member(&self) -> bool {
        let (Some(cli_id), Some(room_id)) = (self.current_clients.0, self.current_room) else {
            return false;
        };
        let Some(room) = self.rooms.iter().find(|room| room.id == room_id) else {
            return false;
        };
        room.is_member(cli_id)
    }

    pub fn advance_action(&mut self, net_handle: &NetworkHandle) -> Result<(), AlysonError> {
        match &self.action_state {
            ActionState::None => {
                let name = self.buf.new_client_name.trim().to_string();
                self.input_mode = InputMode::Selecting;
                if !name.is_empty() {
                    net_handle.spawn_client(name)?;
                }
                self.buf.new_client_name.clear();
            }
            ActionState::CreateChatRoom(id) => {
                let name = self.buf.new_room_name.trim().to_string();
                self.input_mode = InputMode::Selecting;
                if !name.is_empty() {
                    net_handle.create_room(*id, name)?;
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
                            self.errors.push((
                                std::time::Instant::now(),
                                AlysonError::Internal("sender not found".to_string()),
                            ));
                            return Ok(());
                        }
                    };
                    let receiver;
                    let cli_or_room;
                    if matches!(self.action_state, ActionState::SendMessage) {
                        receiver = match self.current_clients.1 {
                            Some(id) => id,
                            None => {
                                self.errors.push((
                                    std::time::Instant::now(),
                                    AlysonError::Internal("receiver not found".to_string()),)
                                );
                                self.input_mode = InputMode::Selecting;
                                return Ok(());
                            }
                        };
                        cli_or_room = true;
                    } else {
                        receiver = match self.current_room {
                            Some(room) => room,
                            None => {
                                self.errors.push((
                                    std::time::Instant::now(),
                                    AlysonError::Internal("receiver not found".to_string()),)
                                );
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
