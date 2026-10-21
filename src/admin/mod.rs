pub mod headless_client;
pub mod load_test;
pub mod metrics;
pub mod server_manager;
pub mod tui;

pub use headless_client::{CancellationToken, headless_connect};
pub use load_test::run_connect_storm;
pub use metrics::{MetricsSnapshot, RollingHistory, ServerMetrics};
pub use server_manager::{ManagedServer, ServerManager, ServerStatus, find_server_binary, start_server};
pub use tui::{AdminApp, InputMode, run_admin_tui};
