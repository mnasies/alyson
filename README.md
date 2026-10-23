# Alyson

A terminal-native chat system and real-time TCP load testing workbench written in Rust.

![Alyson Admin TUI Dashboard](assets/admin-dashboard.png)

Alyson combines a multi-room messaging server and client with an in-terminal admin workbench. You can chat with other users across rooms, or switch to the admin dashboard to spin up child servers, flood them with hundreds of headless bots, and graph connection latency and process resources in real time.

---

## Why I Built This

I wanted to build a real-time messaging system from scratch to understand three things deeply:
1. **Custom binary framing over raw TCP:** Length-prefixed framing with Bincode instead of relying on HTTP or WebSockets.
2. **Async socket multiplexing:** Coordinating hundreds of client streams using Tokio tasks, channels, and SQLite in WAL mode.
3. **Terminal user interfaces:** Building flicker-free, responsive dashboards with Ratatui and Crossterm.

Along the way, verifying concurrency by manually opening 10 terminal windows got old fast. So I built `alyson-admin`: a dedicated load-testing harness that spins up server processes on ephemeral ports, ramps up connect-storms, fires active chat probes to calculate true round-trip latency (RTT) percentiles with HdrHistogram, and graphs everything live using ASCII sparklines.

---

## Features

- **Terminal Chat Client:** Multi-room chat, private direct messaging, speech bubbles, roster sidebar, and persistent history backed by SQLite.
- **Dedicated TCP Server:** Non-blocking Tokio accept loop, length-prefixed binary wire framing, and SQLite in WAL mode for concurrent multi-process reads.
- **Admin Workbench (`alyson-admin`):**
  - **Process Supervision:** Launch, monitor, and cleanly terminate child server processes (`SIGTERM` -> `SIGKILL`).
  - **Connect-Storm Ramping:** Launch hundreds of headless clients at configurable connection rates (e.g. 50/sec) and hold connections open.
  - **Active Chatty Probes:** Headless bots send periodic chat messages and measure true end-to-end round-trip echo latency.
  - **Telemetry & Percentiles:** Microsecond-precision `HdrHistogram` percentiles (p50, p90, p99, p99.9), live CPU% and RSS memory sampling via `sysinfo`.
  - **Ratatui Dashboard:** Live ASCII sparklines for CPU, memory, and active connections, server switcher, help screen overlay, and multi-server aggregate views.
