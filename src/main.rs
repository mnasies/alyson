use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::vec::Vec;

fn handle_clients(mut stream: TcpStream, clients: Arc<Mutex<Vec<TcpStream>>>) {
    let mut buf = [0u8; 1024];
    loop {
        let mut clients_lock = clients.lock().unwrap();
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            Err(_) => {
                println!("Couldn't read message from connected client!");
                0
            }
        };
        if n == 0 {
            let mut i = 0;
            for strm in clients_lock.iter() {
                if strm.peer_addr().unwrap() == stream.peer_addr().unwrap() {
                    break;
                }
                i += 1;
            }
            clients_lock.remove(i);
            break;
        }
        let data = &buf[..n];
        for mut strm in clients_lock.iter() {
            if strm.peer_addr().unwrap() != stream.peer_addr().unwrap() {
                match strm.write_all(&data) {
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

fn main() -> std::io::Result<()> {
    println!("Hello, world!");
    let srvr = TcpListener::bind("127.0.0.1:0")?;

    println!("port number: {}", TcpListener::local_addr(&srvr)?);

    let clients = Arc::new(Mutex::new(Vec::<TcpStream>::new()));
    let mut count_client = 0;

    for stream in srvr.incoming() {
        match stream {
            Ok(s) => {
                count_client += 1;
                println!("Client {} connected", count_client);
                let client_stream = s.try_clone().expect("Failed to clone stream");
                {
                    let mut client_lock = clients.lock().unwrap();
                    client_lock.push(client_stream);
                }

                let clients_clone = Arc::clone(&clients);
                thread::spawn(move || {
                    handle_clients(s, clients_clone);
                });
            }
            Err(_) => {
                println!("Socket accept failure!")
            }
        }
    }
    Ok(())
}
