use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct InboxEntry {
    pub time: SystemTime,
    pub msg: String,
    pub from: usize,       // Client ID
    pub to: usize,         // Client ID or Room ID
    pub cli_or_room: bool, // true = message is from Client (to: ClientID), false = vice-versa
}

impl InboxEntry {
    pub fn new(time: SystemTime, msg: String, from: usize, to: usize, cli_or_room: bool) -> Self {
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
    pub id: usize,
    pub username: String,
    pub cmd_tx: Option<tokio::sync::mpsc::Sender<InboxEntry>>,
    pub writer: tokio::sync::mpsc::Sender<InboxEntry>,
    pub ip: String,
    pub port: u16,
}

impl Client {
    pub fn new(
        id: usize,
        username: String,
        cmd_tx: Option<tokio::sync::mpsc::Sender<InboxEntry>>,
        writer: tokio::sync::mpsc::Sender<InboxEntry>,
        ip: String,
        port: u16,
    ) -> Self {
        Self {
            id,
            username,
            cmd_tx,
            writer,
            ip,
            port,
        }
    }
}
