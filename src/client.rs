use std::net::TcpStream;

#[derive(Debug)]
pub struct Client {
    id: usize,
    username: String,
    pub stream: TcpStream,
}

impl Client {
    pub fn new(id: usize, username: String, stream: TcpStream) -> Self {
        Self {
            id,
            username,
            stream,
        }
    }
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            username: self.username.clone(),
            stream: self.stream.try_clone().unwrap(),
        }
    }
}
