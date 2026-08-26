use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};

fn handle_clients(mut stream: TcpStream) {
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
    println!("Hello, world!");
    let srvr = TcpListener::bind("127.0.0.1:9000")?;

    for stream in srvr.incoming() {
        handle_clients(stream?);
    }
    Ok(())
}
