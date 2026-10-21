use crate::admin::metrics::RollingHistory;
use crate::admin::server_manager::ServerStatus;
use crate::admin::tui::app::{AdminApp, InputMode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Sparkline};
use ratatui::Frame;

/// Renders the complete Alyson Admin TUI dashboard onto the frame.
pub fn render_dashboard(f: &mut Frame, app: &mut AdminApp) {
    let size = f.area();

    // 1. Vertical split: Header (3), Main Body (Min 10), Bottom Bar / Prompt (3)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(size);

    render_header(f, app, chunks[0]);
    render_main(f, app, chunks[1]);
    render_footer(f, app, chunks[2]);
}

/// Renders the top title bar and transient toast notifications.
fn render_header(f: &mut Frame, app: &AdminApp, area: Rect) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(20)])
        .split(area);

    // Title widget
    let title_text = vec![Line::from(vec![
        Span::styled(" ALYSON ADMIN ", Style::default().fg(Color::Cyan).bold()),
        Span::styled("v0.1", Style::default().fg(Color::DarkGray)),
    ])];
    let title_widget = Paragraph::new(title_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(title_widget, header_chunks[0]);

    // Toast / Status widget
    let status_widget = if let Some((msg, is_error)) = app.active_toast() {
        let (icon, color) = if is_error {
            (" ✖ ", Color::Red)
        } else {
            (" ✔ ", Color::Green)
        };
        Paragraph::new(Line::from(vec![
            Span::styled(icon, Style::default().fg(color).bold()),
            Span::styled(msg, Style::default().fg(color)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(color)),
        )
    } else {
        Paragraph::new(Line::from(vec![
            Span::styled(" System State: ", Style::default().fg(Color::DarkGray)),
            Span::styled("Idle / Monitoring", Style::default().fg(Color::Green)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
    };
    f.render_widget(status_widget, header_chunks[1]);
}

/// Renders the primary central area: Server List + Metrics Summary (Left) and Sparklines (Right).
fn render_main(f: &mut Frame, app: &mut AdminApp, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(38), Constraint::Min(30)])
        .split(area);

    // Left column: Servers (top) and Selected Server Details (bottom)
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(main_chunks[0]);

    render_server_list(f, app, left_chunks[0]);
    render_metrics_panel(f, app, left_chunks[1]);

    // Right column: Sparkline trend graphs
    render_graphs(f, app, main_chunks[1]);
}

/// Renders the list of managed child server processes.
fn render_server_list(f: &mut Frame, app: &mut AdminApp, area: Rect) {
    let servers = app.server_manager.list();

    let items: Vec<ListItem> = if servers.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            " No managed servers (press 's' to start)",
            Style::default().fg(Color::DarkGray).italic(),
        )]))]
    } else {
        servers
            .iter()
            .enumerate()
            .map(|(i, server)| {
                let is_selected = i == app.selected_index;
                let cursor = if is_selected { "▶ " } else { "  " };

                let (status_str, status_color) = match &server.status {
                    ServerStatus::Running => ("RUNNING", Color::Green),
                    ServerStatus::Stopped => ("STOPPED", Color::Yellow),
                    ServerStatus::Failed(_) => ("FAILED", Color::Red),
                };

                let line = Line::from(vec![
                    Span::styled(cursor, Style::default().fg(Color::Yellow).bold()),
                    Span::styled(format!("srv-{} ", server.id), Style::default().bold()),
                    Span::styled(
                        format!("[{}] ", status_str),
                        Style::default().fg(status_color).bold(),
                    ),
                    Span::styled(
                        format!("{} (PID {})", server.addr, server.pid),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);

                let style = if is_selected {
                    Style::default().bg(Color::Rgb(30, 40, 60))
                } else {
                    Style::default()
                };

                ListItem::new(line).style(style)
            })
            .collect()
    };

    let list_widget = List::new(items).block(
        Block::default()
            .title(" Managed Servers ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::LightBlue)),
    );
    f.render_widget(list_widget, area);
}

/// Renders live point-in-time metrics of the currently selected server.
fn render_metrics_panel(f: &mut Frame, app: &mut AdminApp, area: Rect) {
    let selected_server = app.selected_server();

    let text = if let Some(server) = selected_server {
        let snap = server.metrics.snapshot();
        let batches_count = app.client_tokens.get(&server.id).map(|b| b.len()).unwrap_or(0);

        vec![
            Line::from(vec![
                Span::styled(" Server ID:        ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("srv-{}", server.id), Style::default().bold()),
            ]),
            Line::from(vec![
                Span::styled(" Active Conns:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", snap.active_connections),
                    Style::default().fg(Color::Green).bold(),
                ),
                Span::styled(
                    format!(" ({} batch groups)", batches_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            Line::from(vec![
                Span::styled(" Total Connects:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}", snap.total_connects), Style::default().bold()),
            ]),
            Line::from(vec![
                Span::styled(" Connect Failures: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", snap.connect_failures),
                    if snap.connect_failures > 0 {
                        Style::default().fg(Color::Red).bold()
                    } else {
                        Style::default().fg(Color::Green)
                    },
                ),
            ]),
            Line::from(vec![
                Span::styled(" Latency (p50):    ", Style::default().fg(Color::DarkGray)),
                Span::styled(snap.connect_latency_p50_str(), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled(" Latency (p99):    ", Style::default().fg(Color::DarkGray)),
                Span::styled(snap.connect_latency_p99_str(), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled(" CPU Utilization:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.1}%", snap.cpu_percent),
                    Style::default().fg(Color::Magenta).bold(),
                ),
            ]),
            Line::from(vec![
                Span::styled(" Memory (RSS):     ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.1} MB", snap.memory_mb()),
                    Style::default().fg(Color::Magenta),
                ),
            ]),
        ]
    } else {
        vec![Line::from(vec![Span::styled(
            " Select a server to view live metrics",
            Style::default().fg(Color::DarkGray).italic(),
        )])]
    };

    let panel = Paragraph::new(text).block(
        Block::default()
            .title(" Metrics Summary ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Blue)),
    );
    f.render_widget(panel, area);
}

/// Renders the 3 sparkline graphs tracking connection and latency history over time.
fn render_graphs(f: &mut Frame, app: &mut AdminApp, area: Rect) {
    let graph_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area);

    let server_id_opt = app.selected_server().map(|s| s.id);

    // Extract history arrays
    let (conn_data, rate_data, lat_data) = if let Some(id) = server_id_opt {
        if let Some(history) = app.history.get(&id) {
            let conns: Vec<u64> = history.iter().map(|s| s.active_connections).collect();
            let rates: Vec<u64> = history.iter().map(|s| s.total_connects).collect();
            let lats: Vec<u64> = history.iter().map(|s| s.connect_latency_p99_us / 1000).collect(); // in ms
            (conns, rates, lats)
        } else {
            (vec![], vec![], vec![])
        }
    } else {
        (vec![], vec![], vec![])
    };

    // 1. Active Connections Sparkline
    let max_conn = conn_data.iter().copied().max().unwrap_or(0);
    let curr_conn = conn_data.last().copied().unwrap_or(0);
    let conn_sparkline = Sparkline::default()
        .block(
            Block::default()
                .title(format!(
                    " Active Connections History [Current: {} | Peak: {}] ",
                    curr_conn, max_conn
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        )
        .data(&conn_data)
        .style(Style::default().fg(Color::Green));
    f.render_widget(conn_sparkline, graph_chunks[0]);

    // 2. Cumulative Connects Sparkline
    let curr_tot = rate_data.last().copied().unwrap_or(0);
    let rate_sparkline = Sparkline::default()
        .block(
            Block::default()
                .title(format!(" Lifetime Total Connects [Total: {}] ", curr_tot))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .data(&rate_data)
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(rate_sparkline, graph_chunks[1]);

    // 3. Latency p99 Sparkline (in ms)
    let max_lat = lat_data.iter().copied().max().unwrap_or(0);
    let curr_lat = lat_data.last().copied().unwrap_or(0);
    let lat_sparkline = Sparkline::default()
        .block(
            Block::default()
                .title(format!(
                    " Latency p99 History [Latest: {}ms | Peak: {}ms] ",
                    curr_lat, max_lat
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .data(&lat_data)
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(lat_sparkline, graph_chunks[2]);
}

/// Renders the bottom footer bar: help keybindings or an active inline prompt.
fn render_footer(f: &mut Frame, app: &AdminApp, area: Rect) {
    match app.input_mode {
        InputMode::Normal => {
            let help_text = Line::from(vec![
                Span::styled(" [s] ", Style::default().fg(Color::Yellow).bold()),
                Span::raw("Start Server  "),
                Span::styled(" [x] ", Style::default().fg(Color::Yellow).bold()),
                Span::raw("Stop Server  "),
                Span::styled(" [b] ", Style::default().fg(Color::Yellow).bold()),
                Span::raw("Batch Launch  "),
                Span::styled(" [d] ", Style::default().fg(Color::Yellow).bold()),
                Span::raw("Disconnect  "),
                Span::styled(" [↑/↓] ", Style::default().fg(Color::Yellow).bold()),
                Span::raw("Select  "),
                Span::styled(" [q/Esc] ", Style::default().fg(Color::Yellow).bold()),
                Span::raw("Quit"),
            ]);
            let help_widget = Paragraph::new(help_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            f.render_widget(help_widget, area);
        }
        InputMode::PromptStartServer => {
            let prompt_text = Line::from(vec![
                Span::styled(" Start Server Address: ", Style::default().fg(Color::Cyan).bold()),
                Span::styled(&app.input_buffer, Style::default().fg(Color::White).bold()),
                Span::styled("▌", Style::default().fg(Color::Yellow)), // cursor
                Span::styled(" (Press Enter to start, Esc to cancel)", Style::default().fg(Color::DarkGray)),
            ]);
            let prompt_widget = Paragraph::new(prompt_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan)),
            );
            f.render_widget(prompt_widget, area);
        }
        InputMode::PromptBatchLaunch => {
            let prompt_text = Line::from(vec![
                Span::styled(" Batch Launch [count rate]: ", Style::default().fg(Color::Magenta).bold()),
                Span::styled(&app.input_buffer, Style::default().fg(Color::White).bold()),
                Span::styled("▌", Style::default().fg(Color::Yellow)), // cursor
                Span::styled(" (e.g. '500 50' -> Enter to launch, Esc to cancel)", Style::default().fg(Color::DarkGray)),
            ]);
            let prompt_widget = Paragraph::new(prompt_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Magenta)),
            );
            f.render_widget(prompt_widget, area);
        }
    }
}
