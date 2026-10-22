use alyson::AlysonError;
use alyson::admin::headless_client::CancellationToken;
use alyson::admin::load_test::{run_chatty, run_connect_storm};
use alyson::admin::metrics::ServerMetrics;
use alyson::admin::server_manager::start_server;
use alyson::admin::tui::run_admin_tui;
use std::env;
use std::sync::Arc;

/// Entry point for the `alyson-admin` binary.
///
/// - Running with no arguments or `admin tui` launches the interactive Ratatui TUI dashboard.
/// - Subcommands `start`, `storm`, `chatty`, and `status` are available for headless/CLI automation.
#[tokio::main]
async fn main() -> Result<(), AlysonError> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        return run_admin_tui().await;
    }

    match args[1].as_str() {
        "tui" => {
            run_admin_tui().await?;
        }
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

            let server_metrics = Arc::clone(&server.metrics);
            let status_task = tokio::spawn(async move {
                let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    let snap = server_metrics.snapshot();
                    println!(
                        "   [Server Stats] CPU: {:>4.1}% | Memory: {:>5.1}MB | Active Conns: {}",
                        snap.cpu_percent,
                        snap.memory_mb(),
                        snap.active_connections
                    );
                }
            });

            tokio::signal::ctrl_c().await.map_err(|e| {
                AlysonError::Internal(format!("Failed to listen for Ctrl+C: {}", e))
            })?;

            status_task.abort();
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

        "chatty" => {
            // Usage: admin chatty <addr> <count> [ramp_rate]
            let addr = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "127.0.0.1:9000".to_string());

            let count: usize = args
                .get(3)
                .and_then(|s| s.parse().ok())
                .unwrap_or(50);

            let ramp_rate: usize = args
                .get(4)
                .and_then(|s| s.parse().ok())
                .unwrap_or(25);

            let metrics = Arc::new(ServerMetrics::new());
            let cancel = CancellationToken::new();

            let cancel_sig = cancel.clone();
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                println!("\n🛑 Received interrupt signal (Ctrl+C). Initiating graceful disconnect...");
                cancel_sig.cancel();
            });

            run_chatty(addr, count, ramp_rate, metrics, cancel).await;
        }

        "status" => {
            println!("ℹ️  alyson-admin status check: ready.");
            println!("Usage: admin start <addr> | admin storm <addr> <N> <rate> | admin chatty <addr> <N> <rate> | admin tui");
        }

        "--version" | "-v" => {
            println!("alyson-admin v0.1.0");
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
    println!("    tui                        Launch the interactive Ratatui TUI dashboard");
    println!("    start <addr>               Launch a child server process (default: 127.0.0.1:9000)");
    println!("    storm <addr> <N> [rate]    Run a connect-storm load test with N clients (default rate: 50/s)");
    println!("    chatty <addr> <N> [rate]   Run an active chatty messaging test with N clients (default rate: 25/s)");
    println!("    status                     Check admin engine status");
    println!("    help                       Print this help message\n");
    println!("EXAMPLES:");
    println!("    cargo run --bin admin");
    println!("    cargo run --bin admin -- start 127.0.0.1:9000");
    println!("    cargo run --bin admin -- storm 127.0.0.1:9000 500 50");
    println!("    cargo run --bin admin -- chatty 127.0.0.1:9000 100 20");
}
