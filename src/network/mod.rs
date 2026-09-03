use crate::WireError;
use crate::client::Client;

use std::io::Write;
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, atomic::Ordering};
use std::thread;

pub struct NetworkHandle {
    pub clients: Arc<Mutex<Vec<Client>>>,
    pub port: Arc<Mutex<Option<u16>>>, // set once run_server binds
    pub next_id: Arc<AtomicUsize>,
}

impl NetworkHandle {
    pub fn new(clients: Arc<Mutex<Vec<Client>>>) -> Self {
        NetworkHandle {
            clients,
            port: Arc::new(Mutex::new(None)),
            next_id: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn run_server(
        port: &str,
        clients: Arc<Mutex<Vec<Client>>>,
        id: Arc<AtomicUsize>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let srvr = TcpListener::bind(port)?;

        // println!("port number: {}", TcpListener::local_addr(&srvr)?);

        let mut count_client = 0;

        for stream in srvr.incoming() {
            match stream {
                Ok(s) => {
                    let id_clone = Arc::clone(&id);
                    let client_stream = s.try_clone().expect("Failed to clone stream");
                    let curr_cli = match perform_handshake(client_stream, id_clone) {
                        Ok(c) => c,
                        Err(e) => {
                            println!("Handshake failure: {:?}", e);
                            continue;
                        }
                    };
                    count_client += 1;
                    println!("Client {}: {} connected", count_client, &curr_cli.username);

                    {
                        let mut client_lock = clients.lock().unwrap();
                        client_lock.push(curr_cli.clone());
                    }

                    let clients_clone = Arc::clone(&clients);
                    thread::spawn(move || {
                        handle_clients(curr_cli, clients_clone);
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

    pub fn spawn_client(name: String) {
        thread::spawn(move || {
            if let Ok(stream) = TcpStream::connect("127.0.0.1:") {
                // send name as your handshake protocol expects, e.g.:
                let _ = (&stream).write_all(format!("{}\n", name).as_bytes());
            }
        });
    }

    pub fn perform_handshake(stream: TcpStream, id: Arc<AtomicUsize>) -> Result<Client, WireError> {
        let username: String = {
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(_) => line.trim().to_string(),
                Err(_) => return Err(WireError::InvalidNameError),
            }
        };
        let client_id = id.fetch_add(1, Ordering::SeqCst);
        Result::<Client, WireError>::Ok(Client::new(client_id, username, stream))
    }
}
