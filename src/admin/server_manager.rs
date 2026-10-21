use crate::AlysonError;
use crate::admin::metrics::ServerMetrics;
use std::env;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessesToUpdate, System};

/// The execution status of a child server process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    /// Process is actively running with the given bound address.
    Running,
    /// Process was gracefully or forcibly stopped by the user/admin.
    Stopped,
    /// Process terminated unexpectedly (e.g., panic, port collision, unhandled signal).
    Failed(String),
}

/// A managed server instance running as an external OS child process.
///
/// # Why Child Processes for Servers?
/// 1. Crash Isolation: If a server encounters a panic or memory leak, the admin TUI stays alive.
/// 2. Clean State: Each server has its own OS process boundary, file descriptors, and SQLite handle.
/// 3. OS Metrics: Tools like `sysinfo` or `/proc/<pid>/stat` require a real OS Process ID (PID).
pub struct ManagedServer {
    /// Unique identifier assigned by the ServerManager.
    pub id: usize,
    /// Handle to the running OS child process.
    pub child: Child,
    /// Process ID (PID) assigned by the operating system kernel.
    pub pid: u32,
    /// The actual socket address the server is listening on (e.g., "127.0.0.1:9000").
    pub addr: String,
    /// Current health / lifecycle status.
    pub status: ServerStatus,
    /// Per-server atomic counters, latency histograms, and system resource metrics.
    pub metrics: Arc<ServerMetrics>,
}

impl ManagedServer {
    /// Poll the child process to check whether it has exited.
    ///
    /// `child.try_wait()` does not block the caller. It performs a non-blocking
    /// `waitpid(pid, WNOHANG)` under the hood on Unix.
    pub fn check_status(&mut self) -> ServerStatus {
        if self.status != ServerStatus::Running {
            return self.status.clone();
        }

        match self.child.try_wait() {
            Ok(Some(exit_status)) => {
                let reason = format!("process exited with {}", exit_status);
                self.status = if exit_status.success() {
                    ServerStatus::Stopped
                } else {
                    ServerStatus::Failed(reason)
                };
            }
            Ok(None) => {
                // None means the process is still running.
                self.status = ServerStatus::Running;
            }
            Err(e) => {
                self.status = ServerStatus::Failed(format!("failed to wait on child: {}", e));
            }
        }

        self.status.clone()
    }

    /// Stop the server process cleanly:
    /// 1. Send SIGTERM so the process has an opportunity to shut down gracefully.
    /// 2. Poll `try_wait()` for up to 5 seconds.
    /// 3. If the process is still alive after the timeout, escalate to SIGKILL.
    pub fn stop(&mut self) -> Result<(), AlysonError> {
        if self.status != ServerStatus::Running {
            return Ok(());
        }

        // 1. Send SIGTERM via libc::kill
        // On Unix, SIGTERM (signal 15) requests polite termination.
        // POSIX: kill(pid, SIGTERM)
        let pid = self.pid as i32;
        let kill_res = unsafe { libc::kill(pid, libc::SIGTERM) };
        if kill_res != 0 {
            let err = std::io::Error::last_os_error();
            // ESRCH (error code 3) means no such process (it might have already exited)
            if err.raw_os_error() != Some(libc::ESRCH) {
                eprintln!("Warning: SIGTERM to PID {} failed: {}", pid, err);
            }
        }

        // 2. Poll for termination with a 5-second deadline
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut exited = false;

        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    exited = true;
                    break;
                }
                Ok(None) => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(_) => break,
            }
        }

        // 3. Fallback to SIGKILL if still running
        if !exited {
            eprintln!(
                "PID {} did not exit within 5s of SIGTERM. Escalating to SIGKILL.",
                self.pid
            );
            let _ = self.child.kill();
            let _ = self.child.wait();
        }

        self.status = ServerStatus::Stopped;
        Ok(())
    }
}

/// Locates the compiled `server` binary executable.
///
/// Looks in:
/// 1. The same directory as the currently executing binary (e.g. `target/debug/`).
/// 2. Common workspace targets: `target/debug/server`, `target/release/server`.
pub fn find_server_binary() -> Result<PathBuf, AlysonError> {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            let candidate = parent.join("server");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    let search_paths = [
        "target/debug/server",
        "target/release/server",
        "./server",
    ];

    for path_str in search_paths {
        let path = Path::new(path_str);
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
    }

    Err(AlysonError::Internal(
        "Could not find compiled 'server' binary. Run `cargo build --bin server` first.".to_string(),
    ))
}

/// Starts a new server child process listening on `addr`.
///
/// Captures stdout and waits for the `"Server started on <addr>"` banner line
/// to confirm successful binding. If the server exits early or panics, captures
/// the error output.
pub fn start_server(id: usize, addr: &str) -> Result<ManagedServer, AlysonError> {
    let binary_path = find_server_binary()?;

    let mut command = Command::new(&binary_path);
    command.args(["serve", addr]);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|e| {
        AlysonError::Internal(format!(
            "Failed to spawn server binary at {}: {}",
            binary_path.display(),
            e
        ))
    })?;

    let pid = child.id();

    // Read stdout line with a timeout to capture the actual bound address.
    // The server outputs: "Server started on <ip:port>"
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AlysonError::Internal("Failed to capture server stdout".to_string()))?;

    // Spawn a helper thread with a channel to read the first line without blocking indefinitely
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let res = reader.read_line(&mut line).map(|_| line);
        let _ = tx.send(res);
    });

    let timeout = Duration::from_secs(4);
    let bound_addr = match rx.recv_timeout(timeout) {
        Ok(Ok(line)) if line.starts_with("Server started on ") => {
            line.trim_start_matches("Server started on ").trim().to_string()
        }
        Ok(Ok(line)) => {
            // Process might have printed something else or failed
            let fallback = if line.trim().is_empty() {
                addr.to_string()
            } else {
                line.trim().to_string()
            };
            fallback
        }
        Ok(Err(e)) => {
            // Check if child exited with error
            let mut err_msg = format!("IO error reading server startup banner: {}", e);
            if let Some(mut stderr) = child.stderr.take() {
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                if !buf.is_empty() {
                    err_msg = buf;
                }
            }
            let _ = child.kill();
            return Err(AlysonError::Internal(err_msg));
        }
        Err(_) => {
            // Timeout waiting for server startup
            let mut err_msg = "Timed out waiting for server to bind and start".to_string();
            if let Some(mut stderr) = child.stderr.take() {
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                if !buf.is_empty() {
                    err_msg = format!("{}: {}", err_msg, buf);
                }
            }
            let _ = child.kill();
            return Err(AlysonError::Internal(err_msg));
        }
    };

    let metrics = Arc::new(ServerMetrics::new());
    spawn_process_sampler(pid, Arc::clone(&metrics));

    Ok(ManagedServer {
        id,
        child,
        pid,
        addr: bound_addr,
        status: ServerStatus::Running,
        metrics,
    })
}

/// Spawns a background task that periodically samples CPU% and Memory (RSS) of a child process.
///
/// Refreshes process stats using `sysinfo` every 1 second and writes them into `ServerMetrics`.
/// Automatically terminates when the child process exits and disappears from the OS process table.
pub fn spawn_process_sampler(pid: u32, metrics: Arc<ServerMetrics>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let sys_pid = Pid::from_u32(pid);
        let mut sys = System::new();
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            sys.refresh_processes(ProcessesToUpdate::Some(&[sys_pid]), true);
            if let Some(process) = sys.process(sys_pid) {
                let cpu = process.cpu_usage() as f64;
                let memory_bytes = process.memory();
                metrics.update_process_stats(cpu, memory_bytes);
            } else {
                // Process is no longer alive in OS table
                break;
            }
        }
    })
}

/// Orchestrates multiple managed servers.
pub struct ServerManager {
    servers: Vec<ManagedServer>,
    next_id: usize,
}

impl Default for ServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerManager {
    /// Initialize an empty ServerManager.
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            next_id: 0,
        }
    }

    /// Launch a new server process on the given address.
    pub fn start(&mut self, addr: &str) -> Result<&ManagedServer, AlysonError> {
        let id = self.next_id;
        self.next_id += 1;

        let server = start_server(id, addr)?;
        self.servers.push(server);
        Ok(self.servers.last().expect("server was just pushed"))
    }

    /// Stop a managed server by its ID.
    pub fn stop(&mut self, id: usize) -> Result<(), AlysonError> {
        if let Some(server) = self.servers.iter_mut().find(|s| s.id == id) {
            server.stop()?;
            Ok(())
        } else {
            Err(AlysonError::Internal(format!("Server with ID {} not found", id)))
        }
    }

    /// Stop all managed servers (used during shutdown / exit).
    pub fn stop_all(&mut self) {
        for server in &mut self.servers {
            let _ = server.stop();
        }
    }

    /// Refresh status of all managed servers and return a reference to the list.
    pub fn list(&mut self) -> &[ManagedServer] {
        for server in &mut self.servers {
            server.check_status();
        }
        &self.servers
    }

    /// Look up a managed server by ID.
    pub fn get_mut(&mut self, id: usize) -> Option<&mut ManagedServer> {
        self.servers.iter_mut().find(|s| s.id == id)
    }
}

impl Drop for ServerManager {
    fn drop(&mut self) {
        // Guarantee no orphaned child server processes remain running on system
        self.stop_all();
    }
}
