use crate::acpx_control;
use crate::agents;
use crate::app::{App, Panel};
use crate::launcher::LauncherStep;
use crate::sessions::SessionStatus;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(7),
            Constraint::Length(1),
        ])
        .split(f.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[0]);

    draw_sessions(f, app, main_chunks[0]);
    draw_events(f, app, main_chunks[1]);
    draw_prompt_composer(f, app, chunks[1]);
    draw_status_bar(f, app, chunks[2]);
    draw_launcher_modal(f, app);
}

fn draw_sessions(f: &mut Frame, app: &mut App, area: Rect) {
    if app.sessions.is_empty() {
        let msg = Paragraph::new(
            "No acpx sessions found.\n\nStart one with: acpx claude \"your prompt\"",
        )
        .block(Block::default().borders(Borders::ALL).title(" Sessions "))
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let status_icon = match s.status {
                SessionStatus::Running => "●",
                SessionStatus::Exited => "○",
                SessionStatus::Closed => "×",
            };
            let status_color = match s.status {
                SessionStatus::Running => Color::Green,
                SessionStatus::Exited => Color::Yellow,
                SessionStatus::Closed => Color::DarkGray,
            };

            let cwd_short = shorten_path(&s.cwd);
            let age = format_age(&s.last_used_at);

            let style = if i == app.selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let agent_info = agents::lookup(&s.agent_type);
            let agent_color = agent_info
                .map(|a| a.display_color)
                .unwrap_or(Color::DarkGray);

            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", status_icon),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    format!("[{}]", s.agent_type),
                    Style::default()
                        .fg(agent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(s.name.as_deref().unwrap_or(&cwd_short).to_string(), style),
            ]);
            let detail_line = if s.name.is_some() {
                format!("  {} · {} · {}", cwd_short, age, s.status)
            } else {
                format!("  {} · {}", age, s.status)
            };
            let detail = Line::from(vec![Span::styled(
                detail_line,
                Style::default().fg(Color::DarkGray),
            )]);

            ListItem::new(vec![line, detail])
        })
        .collect();

    let sessions_block = Block::default()
        .borders(Borders::ALL)
        .border_type(panel_border_type(app.focused_panel == Panel::Sessions))
        .title(" Sessions ")
        .border_style(panel_border_style(app.focused_panel == Panel::Sessions));

    let list = List::new(items)
        .block(sessions_block)
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_events(f: &mut Frame, app: &App, area: Rect) {
    if app.show_details {
        if let Some(s) = app.selected_session() {
            let prompt_target = acpx_control::prompt_session_selector(s);
            let stream_path = s.stream_path.as_deref().unwrap_or("-");
            let name = s.name.as_deref().unwrap_or("-");
            let details = format!(
                "Record ID:     {}\nSession ID:    {}\nName:          {}\nPrompt target: {}\nAgent:         {}\nCWD:           {}\nStatus:        {}\nLast Used:     {}\nStream:        {}",
                s.acpx_record_id,
                s.acp_session_id,
                name,
                prompt_target,
                s.agent_type,
                s.cwd,
                s.status,
                s.last_used_at,
                stream_path
            );
            let paragraph = Paragraph::new(details)
                .block(Block::default().borders(Borders::ALL).title(" Details "))
                .wrap(Wrap { trim: false });
            f.render_widget(paragraph, area);
            return;
        }
    }

    let lines: Vec<Line> = app
        .events
        .iter()
        .map(|e| Line::from(format!("{}", e)))
        .collect();

    let title = if let Some(s) = app.selected_session() {
        format!(
            " Events [{}] ",
            s.acp_session_id.chars().take(8).collect::<String>()
        )
    } else {
        " Events ".to_string()
    };

    let events_block = Block::default()
        .borders(Borders::ALL)
        .border_type(panel_border_type(app.focused_panel == Panel::Events))
        .title(title)
        .border_style(panel_border_style(app.focused_panel == Panel::Events));

    let paragraph = Paragraph::new(lines)
        .block(events_block)
        .wrap(Wrap { trim: true })
        .scroll((app.event_scroll, 0));

    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let bar_bg = Color::Rgb(30, 34, 48);
    let base_style = Style::default().fg(Color::White).bg(bar_bg);

    let mut spans = vec![
        Span::styled(" [Enter]", Style::default().fg(Color::Cyan).bg(bar_bg)),
        Span::styled(" Resume  ", base_style),
        Span::styled("[n]", Style::default().fg(Color::Cyan).bg(bar_bg)),
        Span::styled(" New  ", base_style),
        Span::styled("[Tab]", Style::default().fg(Color::Cyan).bg(bar_bg)),
        Span::styled(" Focus  ", base_style),
        Span::styled("[i/s]", Style::default().fg(Color::Cyan).bg(bar_bg)),
        Span::styled(" Composer  ", base_style),
        Span::styled("[d]", Style::default().fg(Color::Cyan).bg(bar_bg)),
        Span::styled(" Details  ", base_style),
        Span::styled("[r]", Style::default().fg(Color::Cyan).bg(bar_bg)),
        Span::styled(" Refresh  ", base_style),
        Span::styled("[D]", Style::default().fg(Color::Red).bg(bar_bg)),
        Span::styled(" Delete  ", base_style),
        Span::styled("[Ctrl+C]", Style::default().fg(Color::Cyan).bg(bar_bg)),
        Span::styled(" Quit", base_style),
    ];

    if app.launcher_is_active() {
        spans.push(Span::styled(
            "  │  New session: type filter, ↑/↓ select, Enter confirm, Esc cancel",
            Style::default().fg(Color::Yellow).bg(bar_bg),
        ));
    } else if app.confirm_delete {
        if let Some(s) = app.selected_session() {
            let short_id: String = s.acpx_record_id.chars().take(8).collect();
            spans.push(Span::styled(
                "  │  ",
                Style::default().fg(Color::DarkGray).bg(bar_bg),
            ));
            spans.push(Span::styled(
                format!("Delete session {}? ", short_id),
                Style::default()
                    .fg(Color::Red)
                    .bg(bar_bg)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                "(y/n)",
                Style::default().fg(Color::Yellow).bg(bar_bg),
            ));
        }
    } else if let Some(ref msg) = app.status_message {
        spans.push(Span::styled(
            "  │  ",
            Style::default().fg(Color::DarkGray).bg(bar_bg),
        ));
        spans.push(Span::styled(
            msg.clone(),
            Style::default().fg(Color::Yellow).bg(bar_bg),
        ));
    }

    let bar = Paragraph::new(Line::from(spans)).style(base_style);

    f.render_widget(bar, area);
}

fn draw_launcher_modal(f: &mut Frame, app: &App) {
    let Some(launcher) = &app.launcher else {
        return;
    };

    let area = centered_rect(76, 70, f.area());
    f.render_widget(Clear, area);

    let title = match launcher.step {
        LauncherStep::Directory => " New Session: Choose Directory ",
        LauncherStep::Agent => " New Session: Choose Agent ",
    };
    let help = match launcher.step {
        LauncherStep::Directory => "Type to fuzzy-filter directories, or type an existing path",
        LauncherStep::Agent => {
            "Agents are registered/recognized by acpx; launch errors are shown after confirm"
        }
    };
    let selected_dir = launcher
        .selected_directory
        .as_ref()
        .map(|p| shorten_path(&p.display().to_string()))
        .unwrap_or_else(|| "-".to_string());

    let max_rows = area.height.saturating_sub(7).max(1) as usize;
    let rows = launcher.visible_rows(max_rows);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Query: ", Style::default().fg(Color::DarkGray)),
            Span::styled(launcher.current_query(), Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Directory: ", Style::default().fg(Color::DarkGray)),
            Span::styled(selected_dir, Style::default().fg(Color::LightCyan)),
        ]),
        Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
        Line::from(""),
    ];

    if rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matches",
            Style::default().fg(Color::Red),
        )));
    } else {
        for row in rows {
            let marker = if row.selected { "▶ " } else { "  " };
            let label_style = if row.selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Cyan)),
                Span::styled(row.label, label_style),
            ]));
            if let Some(detail) = row.detail {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        truncate_chars(&detail, 80),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }
    }

    let modal = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Thick)
                .title(title)
                .border_style(Style::default().fg(Color::LightCyan)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(modal, area);
}

fn draw_prompt_composer(f: &mut Frame, app: &mut App, area: Rect) {
    let target = app
        .selected_session()
        .map(acpx_control::prompt_session_selector)
        .unwrap_or_else(|| "no session".to_string());
    let short_target = truncate_chars(&target, 32);
    let title = if app.prompt_send_in_flight {
        format!(" Prompt -> {} (sending...) ", short_target)
    } else {
        format!(" Prompt -> {} ", short_target)
    };

    let focused = app.focused_panel == Panel::Prompt;
    let border_style = panel_border_style(focused);

    let placeholder = if focused {
        "Type a prompt. Enter send · Shift+Enter newline · Ctrl+W word · Ctrl+U/K line · ↑/↓ history"
    } else {
        "Press Tab to focus composer, or i/s to jump here"
    };
    app.prompt_editor.set_placeholder(placeholder);
    app.prompt_editor.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(panel_border_type(focused))
            .title(title)
            .border_style(border_style),
    );

    f.render_widget(app.prompt_editor.widget(), area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn shorten_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Some(rest) = path.strip_prefix(home.to_str().unwrap_or("")) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}

fn panel_border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn panel_border_type(focused: bool) -> BorderType {
    if focused {
        BorderType::Thick
    } else {
        BorderType::Plain
    }
}

pub fn format_age(iso: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let ts = parse_iso_timestamp(iso).unwrap_or(now);
    let diff = now.saturating_sub(ts);

    if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

fn parse_iso_timestamp(s: &str) -> Option<u64> {
    let s = s.trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: u64 = parts[0].parse().ok()?;
    let month: u64 = parts[1].parse().ok()?;
    let day: u64 = parts[2].parse().ok()?;

    let time_parts: Vec<&str> = time.split('.').next()?.split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }
    let hour: u64 = time_parts[0].parse().ok()?;
    let min: u64 = time_parts[1].parse().ok()?;
    let sec: u64 = time_parts[2].parse().ok()?;

    // Days from epoch (rough, not accounting for all leap years)
    let days = (year - 1970) * 365
        + (year - 1969) / 4
        + match month {
            1 => 0,
            2 => 31,
            3 => 59,
            4 => 90,
            5 => 120,
            6 => 151,
            7 => 181,
            8 => 212,
            9 => 243,
            10 => 273,
            11 => 304,
            12 => 334,
            _ => 0,
        }
        + day
        - 1;

    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shorten_path_with_home() {
        let home = dirs::home_dir().unwrap();
        let path = format!("{}/workspace/project", home.display());
        let short = shorten_path(&path);
        assert_eq!(short, "~/workspace/project");
    }

    #[test]
    fn test_shorten_path_no_home() {
        assert_eq!(shorten_path("/tmp/project"), "/tmp/project");
    }

    #[test]
    fn test_parse_iso_timestamp() {
        let ts = parse_iso_timestamp("2026-03-14T14:38:58.516Z");
        assert!(ts.is_some());
        let ts = ts.unwrap();
        // Should be roughly 2026-03-14 in seconds since epoch
        assert!(ts > 1_700_000_000); // After 2023
        assert!(ts < 1_900_000_000); // Before 2030
    }

    #[test]
    fn test_parse_iso_timestamp_invalid() {
        assert!(parse_iso_timestamp("not-a-date").is_none());
        assert!(parse_iso_timestamp("").is_none());
    }

    #[test]
    fn test_format_age_recent() {
        // Use a timestamp from right now
        let _now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // We can't easily construct a "now" ISO string, but we can test the function
        // doesn't panic on various inputs
        let age = format_age("2020-01-01T00:00:00Z");
        assert!(age.contains("d ago")); // Should be many days ago
    }

    #[test]
    fn test_agent_color_lookup() {
        use crate::agents;

        let claude = agents::lookup("claude").unwrap();
        assert_eq!(claude.display_color, Color::Magenta);

        let trae = agents::lookup("trae").unwrap();
        assert_eq!(trae.display_color, Color::LightCyan);

        let codex = agents::lookup("codex").unwrap();
        assert_eq!(codex.display_color, Color::Cyan);
    }
}
