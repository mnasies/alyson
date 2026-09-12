use crate::client::InboxEntry;
use crate::ui::app;
use crate::ui::app::{ActionState, App};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn draw_dashboard(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // main area
            Constraint::Length(3), // help text
        ])
        .split(area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // info text area
            Constraint::Percentage(75), // options list, takes remaining space
        ])
        .split(outer[0]);

    draw_client_sidebar(frame, chunks[0], app);
    draw_main_pane(frame, chunks[1], app);

    let help_info =
        Paragraph::new("Use ↑↓←→ to navigate,  press 'Enter' to select, press 'q' to quit")
            .block(Block::default().borders(Borders::ALL));
    frame.render_widget(help_info, outer[1]);
}

fn draw_main_pane(frame: &mut Frame, canvas: Rect, app: &mut App) {
    let block_left = Block::default().title(" WIRE CHAT ").borders(Borders::ALL);

    match app.input_mode {
        app::InputMode::Typing => match &app.action_state {
            ActionState::None => {
                let content = format!("New client name:\n\n{}_", app.buf.new_client_name);
                let info = Paragraph::new(content).block(block_left);
                frame.render_widget(info, canvas);
            }
            ActionState::SendMessage => {
                if let app::DashBoardView::ClientView(id, _) = &app.dashboard_view {
                    draw_client_info(frame, canvas, app, *id);
                }
            }
            _ => {}
        },
        app::InputMode::Selecting => match app.dashboard_view {
            app::DashBoardView::Idle => {
                let content = "Select a client, or choose an action from the sidebar.".to_string();
                let info = Paragraph::new(content).block(block_left);
                frame.render_widget(info, canvas);
            }
            app::DashBoardView::ClientView(id, _) => {
                draw_client_info(frame, canvas, app, id);
                return;
            }
        },
    };
}

fn draw_client_info(frame: &mut Frame, canvas: Rect, app: &mut App, id: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(20), // info text area
            Constraint::Percentage(80), // options list, takes remaining space
        ])
        .split(canvas);
    let client_name = match app.clients.iter().find(|c| c.id == id) {
        Some(c) => c.name.clone(),
        None => "Unknown client".to_string(),
    };
    let upper_block = Block::default()
        .title(client_name.clone())
        .borders(Borders::ALL);

    let ip = match app.clients.iter().find(|c| c.id == id) {
        Some(c) => c.ip.clone(),
        None => "Unknown IP".to_string(),
    };
    let port = match app.clients.iter().find(|c| c.id == id) {
        Some(c) => c.port,
        None => 0,
    };
    let info = format!(
        "Client: {}    IP Address: {}    Port Number: {}",
        client_name.clone(),
        ip,
        port
    );
    let info_block = Paragraph::new(info).block(upper_block);
    frame.render_widget(info_block, chunks[0]);

    match &app.dashboard_view {
        app::DashBoardView::ClientView(_, app::CurrentWindow::ActionList) => {
            let lower_block = Block::default().title(" Actions ").borders(Borders::ALL);

            let actions = vec![
                String::from("Inboxes"),
                String::from("Chat Rooms"),
                String::from("Exit"),
            ];
            let action_list: Vec<ListItem> = actions
                .iter()
                .enumerate()
                .map(|(i, act)| {
                    let style = match app.selected.action_selected {
                        Some(n) => {
                            if i == n {
                                Style::default().add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default()
                            }
                        }
                        None => Style::default(),
                    };
                    let line = Line::from(act.clone()).alignment(Alignment::Left);
                    ListItem::new(line).style(style)
                })
                .collect();
            let actions_list = List::new(action_list).block(lower_block);
            frame.render_widget(actions_list, chunks[1]);
        }
        app::DashBoardView::ClientView(_, app::CurrentWindow::Inbox) => {
            let inbox_chunk = Layout::default()
                .direction(Direction::Horizontal)
                .margin(1)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(chunks[1]);

            let mut clients_clone = app.clients.clone();
            clients_clone.retain(|cli| Some(cli.id) != app.current_clients.0);
            let all_clients_list: Vec<ListItem> = clients_clone
                .iter()
                .enumerate()
                .map(|(i, cli)| {
                    let style = match app.selected.inbox_cli_selected {
                        Some(n) => {
                            if i == n {
                                app.current_clients.1 = Some(cli.id);
                                Style::default().add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default()
                            }
                        }
                        None => Style::default(),
                    };
                    let line = Line::from(cli.name.clone()).alignment(Alignment::Left);
                    ListItem::new(line).style(style)
                })
                .collect();
            let list = List::new(all_clients_list).block(Block::default().borders(Borders::all()));

            frame.render_widget(list, inbox_chunk[0]);

            let outer = Block::default()
                .title(" Send Message ")
                .borders(Borders::ALL);

            let inner_area = outer.inner(inbox_chunk[1]); // area inside the outer border
            frame.render_widget(outer, inbox_chunk[1]); // draw outer box first

            let [messages_area, input_area] =
                Layout::vertical([Constraint::Percentage(80), Constraint::Percentage(20)])
                    .areas(inner_area);

            let (Some(sender_id), Some(receiver_id)) = app.current_clients else {
                let placeholder = Paragraph::new("Select a client to start chatting");
                frame.render_widget(placeholder, messages_area);
                return;
            };

            let mut convo: Vec<&InboxEntry> = app
                .inboxes
                .iter()
                .filter(|e| {
                    (e.from == sender_id && e.to == receiver_id)
                        || (e.from == receiver_id && e.to == sender_id)
                })
                .collect();
            convo.sort_by_key(|e| e.time);

            let max_bubble_width = (messages_area.width as usize).saturating_sub(6).min(40);

            let mut all_lines: Vec<Line> = Vec::new();
            for entry in &convo {
                let is_outgoing = entry.from == sender_id;
                let color = if is_outgoing {
                    Color::Cyan
                } else {
                    Color::Green
                };
                all_lines.extend(build_bubble(
                    &entry.msg,
                    max_bubble_width,
                    color,
                    is_outgoing,
                ));
            }

            let area_height = messages_area.height as usize;
            let total = all_lines.len();

            // Clamp scroll so you can't scroll past the top.
            let max_scroll = total.saturating_sub(area_height);
            app.messages_scroll = app.messages_scroll.min(max_scroll);

            // end_idx moves up as messages_scroll increases; start_idx follows area_height behind it.
            let end_idx = total.saturating_sub(app.messages_scroll);
            let start_idx = end_idx.saturating_sub(area_height);

            let visible_lines = all_lines[start_idx..end_idx].to_vec();

            let messages_list_widget = Paragraph::new(visible_lines);
            frame.render_widget(messages_list_widget, messages_area);

            // input: bordered on top only → reads as a "divider line" not a second box
            let style = if app.selected.inbox_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            let input_block = Block::default()
                .borders(Borders::TOP) // just a line, not a full nested box
                .style(style);

            if matches!(app.input_mode, app::InputMode::Typing) {
                let content = format!("{}_", app.buf.msg_to_client);
                let info = Paragraph::new(content).block(input_block);
                frame.render_widget(info, input_area);
            } else {
                let placeholder = Paragraph::new("Press Enter to type a message");
                frame.render_widget(placeholder.block(input_block), input_area);
            }
        }
        _ => {}
    }
}

fn draw_client_sidebar(frame: &mut Frame, canvas: Rect, app: &mut App) {
    let block_right = Block::default()
        .title(" ALL CLIENTS ")
        .borders(Borders::ALL);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(canvas);

    let clients = &app.clients;
    let client_list: Vec<ListItem> = clients
        .iter()
        .enumerate()
        .map(|(i, cli)| {
            let style = match app.selected.client_selected {
                Some(n) => {
                    if i == n {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    }
                }
                None => Style::default(),
            };
            ListItem::new(cli.name.clone()).style(style)
        })
        .collect();
    let list = List::new(client_list).block(block_right);

    frame.render_widget(list, chunks[0]);

    let options = vec![
        String::from("New Client"),
        String::from("Create New Chat Room"),
    ];
    let cli_opt_list: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = match app.selected.cli_opt_selected {
                Some(n) => {
                    if i == n {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    }
                }
                None => Style::default(),
            };
            ListItem::new(opt.clone()).style(style)
        })
        .collect();
    let list = List::new(cli_opt_list).block(Block::default().borders(Borders::ALL));
    frame.render_widget(list, chunks[1]);
}

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.len() + 1 + word.len() <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current.clone());
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn build_bubble(msg: &str, max_width: usize, color: Color, right: bool) -> Vec<Line<'static>> {
    let wrapped = wrap_text(msg, max_width);
    let content_width = wrapped.iter().map(|l| l.len()).max().unwrap_or(0);

    let top = format!("┌{}┐", "─".repeat(content_width + 2));
    let bottom = format!("└{}┘", "─".repeat(content_width + 2));

    let mut bubble = vec![Line::from(Span::styled(top, Style::default().fg(color)))];
    for l in &wrapped {
        let padded = format!("│ {:<width$} │", l, width = content_width);
        bubble.push(Line::from(Span::styled(padded, Style::default().fg(color))));
    }
    bubble.push(Line::from(Span::styled(bottom, Style::default().fg(color))));

    for line in &mut bubble {
        *line = if right {
            line.clone().right_aligned()
        } else {
            line.clone().left_aligned()
        };
    }
    bubble
}
