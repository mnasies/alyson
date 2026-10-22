use crate::admin::headless_client::{CancellationToken, Scenario, headless_connect};
use crate::admin::metrics::{MetricsSnapshot, RollingHistory, aggregate_snapshots};
use crate::admin::server_manager::{ManagedServer, ServerManager, ServerStatus};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The input mode of the TUI interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal navigation mode (arrow keys, hotkeys: s, x, b, c, a, d, ?, q).
    Normal,
    /// User is entering an address to bind a new server (e.g. "127.0.0.1:9000").
    PromptStartServer,
    /// User is entering batch count and ramp rate with a specific scenario (ConnectStorm or Chatty).
    PromptBatchLaunch(Scenario),
}

/// The state container for the Alyson Admin TUI application.
pub struct AdminApp {
    /// Process orchestrator managing child servers.
    pub server_manager: ServerManager,
    /// Rolling metrics history per server ID (30s window at 250ms interval).
    pub history: HashMap<usize, RollingHistory>,
    /// Active client batch cancellation tokens per server ID.
    pub client_tokens: HashMap<usize, Vec<CancellationToken>>,
    /// Index of the currently highlighted server in the list panel.
    pub selected_index: usize,
    /// Current keyboard input mode.
    pub input_mode: InputMode,
    /// Buffer holding text currently typed by the user in an inline prompt.
    pub input_buffer: String,
    /// Toast notification or error message with an expiration timestamp.
    pub toast: Option<(Instant, String, bool)>, // (timestamp, text, is_error)
    /// Toggle between single server metrics and multi-server total aggregated metrics.
    pub view_aggregate: bool,
    /// Toggle whether the Help Screen modal overlay is visible.
    pub show_help_modal: bool,
    /// Flag signalling the TUI main loop to exit.
    pub should_quit: bool,
    /// Timestamp of the last 250ms metrics snapshot tick.
    pub last_snapshot_tick: Instant,
}

impl Default for AdminApp {
    fn default() -> Self {
        Self::new()
    }
}

impl AdminApp {
    /// Initialize a new, empty AdminApp state.
    pub fn new() -> Self {
        Self {
            server_manager: ServerManager::new(),
            history: HashMap::new(),
            client_tokens: HashMap::new(),
            selected_index: 0,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            toast: None,
            view_aggregate: false,
            show_help_modal: false,
            should_quit: false,
            last_snapshot_tick: Instant::now(),
        }
    }

    /// Retrieve the currently selected server, if any exist.
    pub fn selected_server(&self) -> Option<&ManagedServer> {
        let servers = self.server_manager.servers();
        if servers.is_empty() {
            None
        } else {
            let idx = self.selected_index.min(servers.len().saturating_sub(1));
            Some(&servers[idx])
        }
    }

    /// Move server selection cursor up.
    pub fn select_prev(&mut self) {
        let count = self.server_manager.servers().len();
        if count > 0 {
            if self.selected_index == 0 {
                self.selected_index = count - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    /// Move server selection cursor down.
    pub fn select_next(&mut self) {
        let count = self.server_manager.servers().len();
        if count > 0 {
            self.selected_index = (self.selected_index + 1) % count;
        }
    }

    /// Launch a new managed child server process.
    pub fn start_server(&mut self, addr: &str) {
        let addr_clean = if addr.trim().is_empty() {
            "127.0.0.1:0" // Default to automatic port assignment
        } else {
            addr.trim()
        };

        match self.server_manager.start(addr_clean) {
            Ok(server) => {
                let id = server.id;
                let bound = server.addr.clone();
                let pid = server.pid;
                self.history.insert(id, RollingHistory::new(120));
                self.client_tokens.insert(id, Vec::new());
                self.selected_index = self.server_manager.servers().len().saturating_sub(1);
                self.set_toast(
                    format!("Server srv-{} started on {} (PID {})", id, bound, pid),
                    false,
                );
            }
            Err(e) => {
                self.set_toast(format!("Failed to start server: {}", e), true);
            }
        }
    }

    /// Stop the currently selected managed server.
    pub fn stop_selected_server(&mut self) {
        let id_opt = self.selected_server().map(|s| s.id);
        if let Some(id) = id_opt {
            // Disconnect all headless clients targeting this server first
            self.disconnect_clients(id);

            if let Err(e) = self.server_manager.stop(id) {
                self.set_toast(format!("Error stopping srv-{}: {}", id, e), true);
            } else {
                self.set_toast(format!("srv-{} stopped cleanly.", id), false);
            }
        } else {
            self.set_toast("No server selected.".to_string(), true);
        }
    }

    /// Batch launch N headless clients to the selected server at the specified ramp rate.
    pub fn batch_launch_selected(&mut self, count: usize, ramp_rate: usize, scenario: Scenario) {
        let selected = self
            .selected_server()
            .map(|s| (s.id, s.addr.clone(), s.status.clone(), Arc::clone(&s.metrics)));

        let (server_id, server_addr, metrics) = match selected {
            Some((id, addr, ServerStatus::Running, m)) => (id, addr, m),
            Some((id, _, status, _)) => {
                self.set_toast(format!("srv-{} is not running ({:?})", id, status), true);
                return;
            }
            None => {
                self.set_toast("No server selected.".to_string(), true);
                return;
            }
        };

        let cancel = CancellationToken::new();
        if let Some(tokens) = self.client_tokens.get_mut(&server_id) {
            tokens.push(cancel.clone());
        }

        let ramp_rate = ramp_rate.max(1);
        let interval = Duration::from_secs_f64(1.0 / ramp_rate as f64);

        // Spawn background ramp task
        tokio::spawn(async move {
            for i in 0..count {
                if cancel.is_cancelled() {
                    break;
                }

                let username = format!("bot_{}_{}", server_id, i);
                let addr = server_addr.clone();
                let m = Arc::clone(&metrics);
                let c = cancel.clone();

                tokio::spawn(async move {
                    let _ = headless_connect(addr, username, m, c, scenario).await;
                });

                tokio::time::sleep(interval).await;
            }
        });

        let scenario_label = match scenario {
            Scenario::ConnectStorm => "Connect-Storm",
            Scenario::Chatty => "Chatty",
        };

        self.set_toast(
            format!(
                "Launching {} {} clients to srv-{} at {}/sec",
                count, scenario_label, server_id, ramp_rate
            ),
            false,
        );
    }

    /// Batch launch N headless clients evenly distributed across all running servers.
    pub fn batch_launch_all(&mut self, count: usize, ramp_rate: usize, scenario: Scenario) {
        let running_servers: Vec<(usize, String, Arc<_>)> = self
            .server_manager
            .servers()
            .iter()
            .filter(|s| s.status == ServerStatus::Running)
            .map(|s| (s.id, s.addr.clone(), Arc::clone(&s.metrics)))
            .collect();

        if running_servers.is_empty() {
            self.set_toast("No running servers available for multi-server batch.".to_string(), true);
            return;
        }

        let total_servers = running_servers.len();
        let per_server_count = (count / total_servers).max(1);
        let per_server_rate = (ramp_rate / total_servers).max(1);

        for (id, addr, metrics) in running_servers {
            let cancel = CancellationToken::new();
            if let Some(tokens) = self.client_tokens.get_mut(&id) {
                tokens.push(cancel.clone());
            }

            let interval = Duration::from_secs_f64(1.0 / per_server_rate as f64);
            tokio::spawn(async move {
                for i in 0..per_server_count {
                    if cancel.is_cancelled() {
                        break;
                    }
                    let username = format!("multi_bot_{}_{}", id, i);
                    let a = addr.clone();
                    let m = Arc::clone(&metrics);
                    let c = cancel.clone();

                    tokio::spawn(async move {
                        let _ = headless_connect(a, username, m, c, scenario).await;
                    });
                    tokio::time::sleep(interval).await;
                }
            });
        }

        self.set_toast(
            format!(
                "Launched multi-server batch across {} servers ({} total clients)",
                total_servers, per_server_count * total_servers
            ),
            false,
        );
    }

    /// Disconnect all headless clients targeting the given server ID.
    pub fn disconnect_clients(&mut self, server_id: usize) {
        if let Some(tokens) = self.client_tokens.get_mut(&server_id) {
            let count = tokens.len();
            for token in tokens.drain(..) {
                token.cancel();
            }
            self.set_toast(
                format!("Disconnected all clients on srv-{} ({} batches)", server_id, count),
                false,
            );
        }
    }

    /// Compute an aggregated point-in-time metrics snapshot across all managed servers.
    pub fn aggregate_snapshot(&self) -> MetricsSnapshot {
        let snapshots: Vec<MetricsSnapshot> = self
            .server_manager
            .servers()
            .iter()
            .map(|s| s.metrics.snapshot())
            .collect();

        aggregate_snapshots(&snapshots)
    }

    /// Periodic snapshot tick executed every ~250ms.
    pub fn tick_snapshots(&mut self) {
        if self.last_snapshot_tick.elapsed() >= Duration::from_millis(250) {
            self.last_snapshot_tick = Instant::now();

            let servers = self.server_manager.list();
            for server in servers {
                let id = server.id;
                let snapshot = server.metrics.snapshot();
                self.history
                    .entry(id)
                    .or_insert_with(|| RollingHistory::new(120))
                    .push(snapshot);
            }
        }
    }

    /// Set a user-facing toast notification message with a 4-second lifetime.
    pub fn set_toast(&mut self, text: String, is_error: bool) {
        self.toast = Some((Instant::now(), text, is_error));
    }

    /// Get active toast message text and severity if not expired.
    pub fn active_toast(&self) -> Option<(&str, bool)> {
        if let Some((time, text, is_error)) = &self.toast {
            if time.elapsed() < Duration::from_secs(4) {
                return Some((text.as_str(), *is_error));
            }
        }
        None
    }
}
