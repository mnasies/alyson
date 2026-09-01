mod client;

use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, atomic::Ordering};
use std::thread;
use std::vec::Vec;

use client::Client;
use wire_chat_rs::WireError;

fn handle_clients(curr_client: Client, clients: Arc<Mutex<Vec<Client>>>) {
    let mut buf = [0u8; 1024];
    loop {
        let n = match (&curr_client.stream).read(&mut buf) {
            Ok(n) => n,
            Err(_) => {
                println!("Couldn't read message from connected client!");
                0
            }
        };
        if n == 0 {
            let mut i = 0;
            let mut clients_lock = clients.lock().unwrap();
            for strm in clients_lock.iter() {
                if strm.stream.peer_addr().unwrap() == (&curr_client.stream).peer_addr().unwrap() {
                    break;
                }
                i += 1;
            }
            clients_lock.remove(i);
            break;
        }
        let data = &buf[..n];
        {
            let clients_lock = clients.lock().unwrap();
            for strm in clients_lock.iter() {
                if strm.stream.peer_addr().unwrap() == (&curr_client.stream).peer_addr().unwrap() {
                    continue;
                }
                match (&strm.stream).write_all(&data) {
                    Ok(_) => {}
                    Err(_) => {
                        println!("Writing data to an initiated socket unsuccessful!");
                    }
                }
            }
        }
        println!("{}", String::from_utf8_lossy(&data));
    }
}

fn perform_handshake(mut stream: TcpStream, id: Arc<AtomicUsize>) -> Result<Client, WireError> {
    let username: String = {
        stream.write_all("Enter a username: ".as_bytes()).unwrap();
        let mut buf = [0u8; 1024];
        let temp_name = match stream.read(&mut buf) {
            Ok(n) => String::from_utf8_lossy(&buf[..n]).trim().to_string(),
            Err(_) => return Err(WireError::InvalidNameError),
        };
        temp_name
    };
    let client_id = id.fetch_add(1, Ordering::SeqCst);
    Result::<Client, WireError>::Ok(Client::new(client_id, username, stream))
}

fn main() -> std::io::Result<()> {
    println!("Hello, world!");
    let srvr = TcpListener::bind("127.0.0.1:0")?;

    println!("port number: {}", TcpListener::local_addr(&srvr)?);

    let clients = Arc::new(Mutex::new(Vec::<Client>::new()));
    let id = Arc::new(AtomicUsize::new(0));
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
