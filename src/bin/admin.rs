use alyson::AlysonError;
use alyson::admin::headless_client::CancellationToken;
use alyson::admin::load_test::run_connect_storm;
use alyson::admin::metrics::ServerMetrics;
use alyson::admin::server_manager::start_server;
use std::env;
use std::sync::Arc;

/// Entry point for the `alyson-admin` CLI binary.
///
/// In Phase 1, this provides command-line control to:
/// - Start and manage child server processes.
/// - Connect headless clients.
/// - Run connect-storm load tests with customizable ramp rates.
#[tokio::main]
async fn main() -> Result<(), AlysonError> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "start" => {
            // Usage: admin start [addr]
            let addr = args.get(2).map(String::as_str).unwrap_or("127.0.0.1:9000");
            println!("🚀 Launching server child process at {}...", addr);

            let mut server = start_server(0, addr)?;
            println!(
                "✅ Server is running! ID: {} | PID: {} | Bound Addr: {}",
                server.id, server.pid, server.addr
            );
            println!("Press Ctrl+C to stop the server...");

            tokio::signal::ctrl_c().await.map_err(|e| {
                AlysonError::Internal(format!("Failed to listen for Ctrl+C: {}", e))
            })?;

            println!("\n🛑 Received Ctrl+C. Shutting down server PID {}...", server.pid);
            server.stop()?;
            println!("Server stopped cleanly.");
        }

        "storm" => {
            // Usage: admin storm <addr> <count> [ramp_rate]
            let addr = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "127.0.0.1:9000".to_string());

            let count: usize = args
                .get(3)
                .and_then(|s| s.parse().ok())
                .unwrap_or(100);

            let ramp_rate: usize = args
                .get(4)
                .and_then(|s| s.parse().ok())
                .unwrap_or(50);

            let metrics = Arc::new(ServerMetrics::new());
            let cancel = CancellationToken::new();

            // Set up Ctrl+C handler to cancel the load test cooperatively
            let cancel_sig = cancel.clone();
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                println!("\n🛑 Received interrupt signal (Ctrl+C). Initiating graceful disconnect...");
                cancel_sig.cancel();
            });

            run_connect_storm(addr, count, ramp_rate, metrics, cancel).await;
        }

        "status" => {
            println!("ℹ️  alyson-admin v0.1 status check: ready.");
            println!("In Phase 1 CLI mode, use 'admin start <addr>' to launch a server or 'admin storm <addr> <N> <rate>' to load test.");
        }

        "--version" | "-v" => {
            println!("alyson-admin v0.1.0 (Phase 1)");
        }

        "help" | "--help" | "-h" => {
            print_usage();
        }

        unknown => {
            eprintln!("Unknown command: '{}'", unknown);
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("alyson-admin v0.1 - Admin & Load Testing Tool for Alyson\n");
    println!("USAGE:");
    println!("    admin <COMMAND> [OPTIONS]\n");
    println!("COMMANDS:");
    println!("    start <addr>               Launch a child server process (default: 127.0.0.1:9000)");
    println!("    storm <addr> <N> [rate]    Run a connect-storm load test with N clients (default rate: 50/s)");
    println!("    status                     Check admin engine status");
    println!("    help                       Print this help message\n");
    println!("EXAMPLES:");
    println!("    cargo run --bin admin -- start 127.0.0.1:9000");
    println!("    cargo run --bin admin -- storm 127.0.0.1:9000 500 50");
}
