use crate::WireError;
use crate::client::Client;
use crate::client::InboxEntry;

use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum NetworkCommand {
    SpawnClient { username: String },
    SendMessage { client_id: usize, msg: String },
}

#[derive(Debug)]
pub enum NetworkEvent {
    ClientConnected {
        id: usize,
        username: String,
        ip: String,
        port: u16,
    },
    ClientDisconnected {
        id: usize,
    },
    MessageReceived(InboxEntry),
    ErrorOccurred(WireError),
}

use std::io::Write;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, atomic::Ordering};
use std::thread;

#[derive(Clone)]
pub struct NetworkHandle {
    pub clients: Arc<Mutex<Vec<Client>>>,
    pub port: Arc<Mutex<Option<String>>>, // set once run_server binds
    pub next_id: Arc<AtomicUsize>,
    pub client_ports: Arc<Mutex<Vec<String>>>,
    pub errors: Arc<Mutex<Vec<(Instant, WireError)>>>,
    pub inboxes: Arc<Mutex<Vec<InboxEntry>>>,
    pub outgoing: Arc<Mutex<Vec<InboxEntry>>>,
}

impl NetworkHandle {
    pub fn new() -> Self {
        NetworkHandle {
            clients: Arc::new(Mutex::new(Vec::new())),
            port: Arc::new(Mutex::new(None)),
            next_id: Arc::new(AtomicUsize::new(0)),
            client_ports: Arc::new(Mutex::new(Vec::new())),
            errors: Arc::new(Mutex::new(Vec::new())),
            inboxes: Arc::new(Mutex::new(Vec::new())),
            outgoing: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn run_server(&mut self) -> Result<(), WireError> {
        let srvr = TcpListener::bind("127.0.0.1:0")?;
        let port = TcpListener::local_addr(&srvr)?.to_string();
        *self.port.lock().unwrap() = Some(port);

        for stream in srvr.incoming() {
            match stream {
                Ok(s) => {
                    let client_stream = s.try_clone().expect("Failed to clone stream");
                    let curr_cli = match Self::perform_handshake(self, client_stream) {
                        Ok(c) => c,
                        Err(e) => {
                            self.errors.lock().unwrap().push((Instant::now(), e));
                            continue;
                        }
                    };

                    {
                        let mut client_lock = self.clients.lock().unwrap();
                        client_lock.push(curr_cli.clone());
                        // println!("run_server: clients now = {}", client_lock.len());
                    }

                    let clients_clone = Arc::clone(&self.clients);
                    thread::spawn(move || {
                        Self::handle_clients(curr_cli, clients_clone);
                    });
                }
                Err(e) => {
                    return Err(WireError::TcpConnectionFailed(e));
                }
            }
        }
        Ok(())
    }

    pub fn handle_clients(curr_client: Client, clients: Arc<Mutex<Vec<Client>>>) {
        let binding = &curr_client.stream.try_clone().unwrap();
        let mut reader = BufReader::new(binding);
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = match reader.read_line(&mut line) {
                Ok(n) => n,
                Err(_) => {
                    println!("Couldn't read message from connected client!");
                    0
                }
            };
            if bytes_read == 0 {
                let mut i = 0;
                let mut clients_lock = clients.lock().unwrap();
                for strm in clients_lock.iter() {
                    if strm.stream.peer_addr().unwrap()
                        == (&curr_client.stream).peer_addr().unwrap()
                    {
                        break;
                    }
                    i += 1;
                }
                clients_lock.remove(i);
                break;
            }
            let msg = format!("{}\n", &line.trim_end());
            let msg_bytes = &msg.as_bytes();
            {
                let clients_lock = clients.lock().unwrap();
                for temp_client in clients_lock.iter() {
                    if temp_client.id == curr_client.id {
                        continue;
                    }
                    match (&temp_client.stream).write_all(&msg_bytes) {
                        Ok(_) => {}
                        Err(_) => {
                            println!("Writing data to an initiated socket unsuccessful!");
                        }
                    }
                }
            }
            println!("{}", &msg);
        }
    }

    fn resolve_client_id(&self, local_port: u16) -> Result<usize, WireError> {
        let timeout = Duration::from_millis(500);
        let start = Instant::now();
        loop {
            if let Some(client) = self
                .clients
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.port == local_port)
            {
                return Ok(client.id);
            }
            if start.elapsed() > timeout {
                return Err(WireError::ClientRegistrationTimeout);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn spawn_client(&mut self, name: String) -> Result<(), WireError> {
        match self.port.lock().unwrap().clone() {
            Some(port) => {
                let mut stream = TcpStream::connect(&port)?;

                // eprintln!("spawn_client: got port {}", port);
                let client_ports_clone = Arc::clone(&self.client_ports);
                let inboxes = Arc::clone(&self.inboxes);
                let mut reader = BufReader::new(stream.try_clone()?);

                let msg = format!("{}\n", name);
                stream.write_all(msg.as_bytes()).unwrap();
                let addr = stream.local_addr()?;
                client_ports_clone.lock().unwrap().push(addr.to_string());
                let cli_port = addr.port();

                let id = self.resolve_client_id(cli_port)?;

                thread::spawn(move || {
                    // eprintln!("spawn_client: connected");

                    let mut line = String::new();
                    loop {
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) => break, // connection closed
                            Ok(_) => {
                                inboxes.lock().unwrap().push(InboxEntry {
                                    time: Instant::now(),
                                    msg: line.trim().to_string(),
                                    from: id, // or parse sender from the wire message
                                    to: id,
                                    cli_or_room: true,
                                });
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
            None => return Err(WireError::PortNotAvailable),
        };
        Ok(())
    }

    pub fn perform_handshake(&mut self, stream: TcpStream) -> Result<Client, WireError> {
        let username: String = {
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(_) => {
                    let name = line.trim().to_string();
                    name
                }
                Err(_) => {
                    return Err(WireError::InvalidName);
                }
            }
        };
        let client_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let ip = stream.peer_addr()?.ip().to_string();
        let port = stream.peer_addr()?.port();
        Result::<Client, WireError>::Ok(Client::new(client_id, username, stream, ip, port))
    }
}
