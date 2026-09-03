use std::net::TcpStream;

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
