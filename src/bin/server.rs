use alyson::WireError;
use alyson::network::server_side::run_server;
use std::env;
use std::panic;

#[tokio::main]
async fn main() -> Result<(), WireError> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("serve") => {
            let addr = match args.get(2).cloned() {
                Some(addr) => addr,
                None => "127.0.0.1:0".to_string(),
            };
            run_server(addr.clone()).await?;
        }
        _ => {
            panic!("unknown command: Usage `-- serve [addr]`");
        }
    }

    Ok(())
}
