use crate::agents;
use crate::app::App;
use crate::sessions::SessionStatus;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[0]);

    draw_sessions(f, app, main_chunks[0]);
    draw_events(f, app, main_chunks[1]);
    draw_status_bar(f, app, chunks[1]);
}

fn draw_sessions(f: &mut Frame, app: &App, area: Rect) {
    if app.sessions.is_empty() {
        let msg = Paragraph::new("No acpx sessions found.\n\nStart one with: acpx claude \"your prompt\"")
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
                    Style::default().fg(agent_color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(cwd_short, style),
            ]);
            let detail = Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{} · {}", age, s.status),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            ListItem::new(vec![line, detail])
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Sessions "),
    );

    f.render_widget(list, area);
}

fn draw_events(f: &mut Frame, app: &App, area: Rect) {
    if app.show_details {
        if let Some(s) = app.selected_session() {
            let details = format!(
                "Record ID:  {}\nSession ID: {}\nAgent:      {}\nCWD:        {}\nStatus:     {}\nLast Used:  {}",
                s.acpx_record_id, s.acp_session_id, s.agent_type, s.cwd, s.status, s.last_used_at
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

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    if let Some(ref msg) = app.status_message {
        let bar = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", msg),
                Style::default().fg(Color::Yellow),
            ),
        ]));
        f.render_widget(bar, area);
        return;
    }

    let bar = Paragraph::new(Line::from(vec![
        Span::styled(" [Enter]", Style::default().fg(Color::Cyan)),
        Span::raw(" Resume  "),
        Span::styled("[d]", Style::default().fg(Color::Cyan)),
        Span::raw(" Details  "),
        Span::styled("[r]", Style::default().fg(Color::Cyan)),
        Span::raw(" Refresh  "),
        Span::styled("[q]", Style::default().fg(Color::Cyan)),
        Span::raw(" Quit"),
    ]));

    f.render_widget(bar, area);
}

pub fn shorten_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Some(rest) = path.strip_prefix(home.to_str().unwrap_or("")) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
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
    let days = (year - 1970) * 365 + (year - 1969) / 4
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
