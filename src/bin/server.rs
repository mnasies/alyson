use std::panic;

use alyson::WireError;
use alyson::network::server_side::run_server;

#[tokio::main]
async fn main() -> Result<(), WireError> {
    match args.get(1).map(String::as_str) {
        Some("serve") => {
            let addr = match args.get(2).cloned() {
                Some(addr) => addr,
                None => "127.0.0.1:0".to_string(),
            };
            tokio::spawn(run_server(addr.as_str()));
        }
        _ => {
            panic!("unknown command: Usage `-- serve [addr]`");
        }
    }

    Ok(())
}
