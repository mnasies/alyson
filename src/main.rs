use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::vec::Vec;

fn handle_clients(mut stream: TcpStream, mut clients: Arc<Mutex<Vec<TcpStream>>>) {
    let mut buf = [0u8; 1024];
    loop {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        let data = &buf[..n];
        println!("{}", String::from_utf8_lossy(&data));
        stream.write_all(&data).unwrap();
    }
}

fn main() -> std::io::Result<()> {
    let clients = Arc::new(Mutex::new(Vec::<TcpStream>::new()));
    println!("Hello, world!");
    let srvr = TcpListener::bind("127.0.0.1:9000")?;
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
