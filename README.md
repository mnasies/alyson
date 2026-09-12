# Alyson

Alyson is a terminal-based multi-client TCP inspector. It is a `netcat` replacement with a UI.

Alyson lets you spin up multiple TCP clients against a local server,
watch connections in real time, and send messages between them from a
single TUI. 
It also let's you create chat rooms and send messages between multiple clients.

## Features

- Multi-client TCP server with a live dashboard of connected clients
- Send messages between any two connected clients
- Per-client conversation view (chat-style, sender/receiver aligned)
- Robust error handling — recoverable errors show as toast notifications,
  unrecoverable ones fail safely without corrupting terminal state
- (in progress) Cross-machine hosting — one PC hosts, another joins over LAN

## Running

```bash
cargo run
```

## Architecture

Built on `tokio` async tasks and `ratatui` for the TUI.
