use crate::WireError;
use crate::client::Client;

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
}

impl NetworkHandle {
    pub fn new(clients: Arc<Mutex<Vec<Client>>>) -> Self {
        NetworkHandle {
            clients,
            port: Arc::new(Mutex::new(None)),
            next_id: Arc::new(AtomicUsize::new(0)),
            client_ports: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn run_server(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let srvr = TcpListener::bind("127.0.0.1:0")?;
        let port = TcpListener::local_addr(&srvr)?.to_string();
        *self.port.lock().unwrap() = Some(port);

        let mut count_client = 0;

        for stream in srvr.incoming() {
            match stream {
                Ok(s) => {
                    let client_stream = s.try_clone().expect("Failed to clone stream");
                    let curr_cli = match Self::perform_handshake(self, client_stream) {
                        Ok(c) => c,
                        Err(e) => {
                            println!("Handshake failure: {:?}", e);
                            continue;
                        }
                    };
                    count_client += 1;
                    println!("Client {}: {} connected", count_client, &curr_cli.username);

                    {
                        let mut client_lock = self.clients.lock().unwrap();
                        client_lock.push(curr_cli.clone());
                        println!("run_server: clients now = {}", client_lock.len());
                    }

                    let clients_clone = Arc::clone(&self.clients);
                    thread::spawn(move || {
                        Self::handle_clients(curr_cli, clients_clone);
                    });
                }
                Err(_) => {
                    println!("Socket accept failure!")
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

    pub fn spawn_client(&mut self, name: String) {
        match self.port.lock().unwrap().clone() {
            Some(port) => {
                // eprintln!("spawn_client: got port {}", port);
                let client_ports_clone = Arc::clone(&self.client_ports);
                thread::spawn(move || match TcpStream::connect(&port) {
                    Ok(mut stream) => {
                        // eprintln!("spawn_client: connected");
                        let msg = format!("{}\n", name);
                        stream.write_all(msg.as_bytes()).unwrap();
                        let cli_port = stream.local_addr().unwrap().to_string();
                        client_ports_clone.lock().unwrap().push(cli_port);

                        loop {
                            thread::sleep(std::time::Duration::from_secs(3600));
                        }
                    }
                    Err(e) => eprintln!("spawn_client: connect FAILED: {e}"),
                });
            }
            None => return,
        };
    }

    pub fn perform_handshake(&mut self, stream: TcpStream) -> Result<Client, WireError> {
        let username: String = {
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(_) => {
                    let name = line.trim().to_string();
                    println!("Username: {}", name);
                    name
                }
                Err(_) => {
                    eprintln!("Invalid Name");
                    return Err(WireError::InvalidNameError);
                }
            }
        };
        let client_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        Result::<Client, WireError>::Ok(Client::new(client_id, username, stream))
    }
}
