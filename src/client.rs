use std::net::TcpStream;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct InboxEntry {
    pub time: Instant,
    pub msg: String,
    pub from: usize,       // Client ID
    pub to: usize,         // Client ID or Room ID
    pub cli_or_room: bool, // true = message is from Client (to: ClientID), false = vice-versa
}

impl InboxEntry {
    pub fn new(time: Instant, msg: String, from: usize, to: usize, cli_or_room: bool) -> Self {
        InboxEntry {
            time,
            msg,
            from,
            to,
            cli_or_room,
        }
    }
}

#[derive(Debug)]
pub struct Client {
    pub id: usize,
    pub username: String,
    pub stream: TcpStream,
    pub ip: String,
    pub port: u16,
}

impl Client {
    pub fn new(id: usize, username: String, stream: TcpStream, ip: String, port: u16) -> Self {
        Self {
            id,
            username,
            stream,
            ip,
            port,
        }
    }
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            username: self.username.clone(),
            stream: self.stream.try_clone().unwrap(),
            ip: self.ip.clone(),
            port: self.port,
        }
    }
}
