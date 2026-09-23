use crate::WireError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::SystemTime;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientInfo {
    pub id: u64,
    pub username: String,
    pub ip: String,
    pub port: u16,
}

impl ClientInfo {
    pub fn new(id: u64, username: String, ip: String, port: u16) -> Self {
        ClientInfo {
            id,
            username,
            ip,
            port,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientRequest {
    CreateRoom { username: String },
    JoinRoom { client_id: u64, room_id: u64 },
    ChatMessage(InboxEntry),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerEvent {
    Welcome { id: usize },
    Roster(Vec<ClientInfo>),
    PeerJoined(ClientInfo),
    ClientDisconnected(u64),
    PeerLeft(usize),
    ChatMessage(InboxEntry),
    RoomCreated(Room),
    RoomJoined(Room),
    Error(String),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct InboxEntry {
    pub time: SystemTime,
    pub msg: String,
    pub from: u64,         // Client ID
    pub to: u64,           // Client ID or Room ID
    pub cli_or_room: bool, // true = message is from Client (to: ClientID), false = vice-versa
}

impl InboxEntry {
    pub fn new(time: SystemTime, msg: String, from: u64, to: u64, cli_or_room: bool) -> Self {
        InboxEntry {
            time,
            msg,
            from,
            to,
            cli_or_room,
        }
    }
}

impl std::fmt::Display for InboxEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    pub id: u64,
    pub username: String,
    pub cmd_tx: Option<Sender<WireMessage>>,
    pub writer: Option<Sender<WireMessage>>,
    pub ip: String,
    pub port: u16,
}

impl Client {
    pub fn new(
        id: u64,
        username: String,
        cmd_tx: Option<Sender<WireMessage>>,
        writer: Option<Sender<WireMessage>>,
        ip: String,
        port: u16,
    ) -> Self {
        Client {
            id,
            username,
            cmd_tx,
            writer,
            ip,
            port,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: u64,
    pub username: String,
    pub members: HashSet<u64>,
}

impl Room {
    pub fn new(id: u64, username: String) -> Self {
        Self {
            id,
            username,
            members: HashSet::new(),
        }
    }

    pub fn is_member(&self, id: u64) -> bool {
        match self.members.iter().find(|n| **n == id) {
            Some(_) => true,
            None => false,
        }
    }

    // if the passed client_id already existed it does nothing
    pub fn add_member(&mut self, id: u64) {
        self.members.insert(id);
    }
}
