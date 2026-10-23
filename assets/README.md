# Assets Guide

This directory holds visual assets for the Alyson repository and documentation.

## Required Asset Files

Please drop your screenshots or recordings into this folder using the exact filenames below:

### 1. `admin-dashboard.png`
- **Description:** Screenshot of the interactive `alyson-admin` Ratatui TUI dashboard.
- **Recommended capture:** Run `cargo run --bin admin`, press `s` + Enter to start a server, then press `b` + Enter to launch a 100-client storm. Capture the dashboard showing the active server in the sidebar, CPU/Memory/Connection sparklines, and latency percentile bars.
- **Suggested resolution:** ~1280x720 or clean terminal window screenshot.

### 2. `client-chat.png`
- **Description:** Screenshot of the `alyson` messaging client TUI.
- **Recommended capture:** Run `cargo run --bin client -- connect 127.0.0.1:9000 --name Alice`, join a room, and send a few chat messages showing the chat bubbles, sidebar user roster, and input buffer.
- **Suggested resolution:** ~1280x720 or clean terminal window screenshot.

### 3. `load-test-demo.gif` (Optional / Recommended)
- **Description:** Animated GIF or short clip showing a live load test running in the terminal.
- **Recommended capture:** Running `cargo run --bin admin -- chatty 127.0.0.1:9000 50 25` showing the 1-second live telemetry output and summary report.
