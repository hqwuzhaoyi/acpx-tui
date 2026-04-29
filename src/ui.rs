use crate::acpx_control;
use crate::agents;
use crate::app::{App, Panel};
use crate::events::DisplayEvent;
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

    let lines = events_lines(&app.events);

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

const CONTINUATION_GUTTER: &str = "   ";

fn events_lines(events: &[DisplayEvent]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut previous_event: Option<&DisplayEvent> = None;

    for event in events {
        if should_separate_events(previous_event, event) {
            lines.push(Line::from(""));
        }
        lines.extend(event_lines(event));
        previous_event = Some(event);
    }

    lines
}

fn should_separate_events(previous: Option<&DisplayEvent>, current: &DisplayEvent) -> bool {
    matches!(
        (previous, current),
        (Some(DisplayEvent::UserMessage(_)), DisplayEvent::Message(_))
            | (Some(DisplayEvent::Message(_)), DisplayEvent::UserMessage(_))
    )
}

fn event_lines(event: &DisplayEvent) -> Vec<Line<'static>> {
    match event {
        DisplayEvent::Message(text) => assistant_message_lines(text),
        DisplayEvent::UserMessage(text) => user_prompt_lines(text),
        DisplayEvent::Thinking(text) => markdownish_event_lines("💭 ", text, thinking_style()),
        DisplayEvent::ToolCall { .. } | DisplayEvent::Usage { .. } => {
            vec![Line::from(format!("{}", event))]
        }
    }
}

fn assistant_message_lines(text: &str) -> Vec<Line<'static>> {
    markdownish_event_lines("", text, message_style())
}

fn user_prompt_lines(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for raw_line in text.lines() {
        if lines.is_empty() && raw_line.trim().is_empty() {
            continue;
        }

        let gutter = if lines.is_empty() { "❯ " } else { "  " };
        let marker_style = if lines.is_empty() {
            prompt_marker_style()
        } else {
            prompt_bar_style()
        };
        let mut spans = vec![Span::styled(gutter.to_string(), marker_style)];
        spans.extend(inline_spans(raw_line, prompt_bar_style()));
        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "❯ ".to_string(),
            prompt_marker_style(),
        )));
    }

    lines
}

fn markdownish_event_lines(
    icon_gutter: &'static str,
    text: &str,
    base_style: Style,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut in_plain_paragraph = false;

    for raw_line in text.lines() {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            in_plain_paragraph = false;
            continue;
        }

        if !in_code_block && is_table_separator(raw_line) {
            in_plain_paragraph = false;
            continue;
        }

        if lines.is_empty() && raw_line.trim().is_empty() {
            continue;
        }

        if raw_line.trim().is_empty() {
            lines.push(Line::from(""));
            in_plain_paragraph = false;
            continue;
        }

        let gutter = if icon_gutter.is_empty() {
            ""
        } else if lines.is_empty() {
            icon_gutter
        } else {
            CONTINUATION_GUTTER
        };

        lines.push(render_markdownish_line(
            gutter,
            raw_line,
            base_style,
            in_code_block,
            in_plain_paragraph,
        ));
        in_plain_paragraph = !in_code_block && is_plain_paragraph_line(raw_line);
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(icon_gutter, gutter_style())));
    }

    lines
}

fn render_markdownish_line(
    gutter: &str,
    raw_line: &str,
    base_style: Style,
    in_code_block: bool,
    in_plain_paragraph: bool,
) -> Line<'static> {
    if in_code_block {
        let mut spans = Vec::new();
        if !gutter.is_empty() {
            spans.push(Span::styled(gutter.to_string(), gutter_style()));
        }
        spans.push(Span::styled(raw_line.to_string(), code_style()));
        return Line::from(spans);
    }

    let (indent, rest) = split_indent(raw_line);

    if let Some(heading) = heading_text(rest) {
        let mut spans = Vec::new();
        if !gutter.is_empty() {
            spans.push(Span::styled(gutter.to_string(), gutter_style()));
        }
        spans.push(Span::styled(indent.to_string(), gutter_style()));
        let marker_len = rest.len() - heading.len();
        spans.push(Span::styled(rest[..marker_len].to_string(), marker_style()));
        spans.extend(inline_spans(heading, heading_style()));
        return Line::from(spans);
    }

    if is_table_row(rest) {
        let mut spans = Vec::new();
        if !gutter.is_empty() {
            spans.push(Span::styled(gutter.to_string(), gutter_style()));
        }
        spans.push(Span::styled(indent.to_string(), gutter_style()));
        spans.extend(table_spans(rest, base_style));
        return Line::from(spans);
    }

    if let Some(body) = rest.strip_prefix("- ").or_else(|| rest.strip_prefix("* ")) {
        let mut spans = Vec::new();
        if !gutter.is_empty() {
            spans.push(Span::styled(gutter.to_string(), gutter_style()));
        }
        spans.push(Span::styled(indent.to_string(), gutter_style()));
        spans.push(Span::styled(rest[..2].to_string(), marker_style()));
        spans.extend(inline_spans(body, base_style));
        return Line::from(spans);
    }

    if let Some((marker, body)) = ordered_list_parts(rest) {
        let mut spans = Vec::new();
        if !gutter.is_empty() {
            spans.push(Span::styled(gutter.to_string(), gutter_style()));
        }
        spans.push(Span::styled(indent.to_string(), gutter_style()));
        spans.push(Span::styled(marker.to_string(), marker_style()));
        spans.extend(inline_spans(body, base_style));
        return Line::from(spans);
    }

    if let Some(body) = rest.strip_prefix('>') {
        let body = body.strip_prefix(' ').unwrap_or(body);
        let mut spans = Vec::new();
        if !gutter.is_empty() {
            spans.push(Span::styled(gutter.to_string(), gutter_style()));
        }
        spans.push(Span::styled(indent.to_string(), gutter_style()));
        spans.push(Span::styled("│ ".to_string(), marker_style()));
        spans.extend(inline_spans(body, quote_style()));
        return Line::from(spans);
    }

    let mut spans = if gutter.is_empty() {
        if in_plain_paragraph {
            vec![Span::styled("  ".to_string(), marker_style())]
        } else {
            vec![Span::styled("• ".to_string(), marker_style())]
        }
    } else {
        vec![Span::styled(gutter.to_string(), gutter_style())]
    };
    spans.extend(inline_spans(raw_line, base_style));
    Line::from(spans)
}

fn is_plain_paragraph_line(raw_line: &str) -> bool {
    let (_, rest) = split_indent(raw_line);
    !raw_line.trim().is_empty()
        && heading_text(rest).is_none()
        && !is_table_row(rest)
        && rest
            .strip_prefix("- ")
            .or_else(|| rest.strip_prefix("* "))
            .is_none()
        && ordered_list_parts(rest).is_none()
        && !rest.starts_with('>')
}

fn heading_text(s: &str) -> Option<&str> {
    let level = s.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&level) {
        let rest = &s[level..];
        if rest.starts_with(' ') {
            return Some(rest.trim_start());
        }
    }
    None
}

fn ordered_list_parts(s: &str) -> Option<(&str, &str)> {
    let dot = s.find(". ")?;
    if dot == 0 || !s[..dot].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((&s[..dot + 2], &s[dot + 2..]))
}

fn is_table_row(s: &str) -> bool {
    let trimmed = s.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.matches('|').count() >= 2
}

fn is_table_separator(s: &str) -> bool {
    let trimmed = s.trim();
    if !is_table_row(trimmed) {
        return false;
    }

    trimmed
        .trim_matches('|')
        .split('|')
        .all(|cell| cell.trim().chars().all(|c| matches!(c, '-' | ':' | ' ')))
}

fn table_spans(row: &str, base_style: Style) -> Vec<Span<'static>> {
    let cells: Vec<&str> = row
        .trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    let mut spans = Vec::new();

    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" │ ".to_string(), marker_style()));
        }
        spans.extend(inline_spans(cell, base_style));
    }

    spans
}

fn split_indent(s: &str) -> (&str, &str) {
    let first_non_ws = s
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(idx, _)| idx)
        .unwrap_or(s.len());
    s.split_at(first_non_ws)
}

fn inline_spans(text: &str, base_style: Style) -> Vec<Span<'static>> {
    inline_spans_with_style(text, base_style)
}

fn inline_spans_with_style(text: &str, base_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;

    loop {
        let backtick = remaining.find('`');
        let strong = remaining.find("**");
        let next = match (backtick, strong) {
            (Some(backtick), Some(strong)) => {
                if backtick < strong {
                    Some((backtick, InlineMarker::Code))
                } else {
                    Some((strong, InlineMarker::Strong))
                }
            }
            (Some(backtick), None) => Some((backtick, InlineMarker::Code)),
            (None, Some(strong)) => Some((strong, InlineMarker::Strong)),
            (None, None) => None,
        };

        let Some((start, marker)) = next else {
            append_plain_spans(&mut spans, remaining, base_style);
            return spans;
        };

        let (before, after_start) = remaining.split_at(start);
        append_plain_spans(&mut spans, before, base_style);

        match marker {
            InlineMarker::Code => {
                let after_tick = &after_start[1..];
                let Some(end) = after_tick.find('`') else {
                    append_plain_spans(&mut spans, after_start, base_style);
                    return spans;
                };

                let (code, after_code) = after_tick.split_at(end);
                spans.push(Span::styled(code.to_string(), code_style()));
                remaining = &after_code[1..];
            }
            InlineMarker::Strong => {
                let after_marker = &after_start[2..];
                let Some(end) = after_marker.find("**") else {
                    append_plain_spans(&mut spans, after_start, base_style);
                    return spans;
                };

                let (strong, after_strong) = after_marker.split_at(end);
                spans.extend(inline_spans_with_style(strong, strong_style(base_style)));
                remaining = &after_strong[2..];
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum InlineMarker {
    Code,
    Strong,
}

fn append_plain_spans(spans: &mut Vec<Span<'static>>, text: &str, base_style: Style) {
    if text.is_empty() {
        return;
    }

    let mut pending = String::new();
    for part in text.split_inclusive(char::is_whitespace) {
        let (token, trailing_ws) = split_trailing_whitespace(part);
        if token.is_empty() {
            pending.push_str(trailing_ws);
            continue;
        }

        if let Some((leading, core, trailing)) = codeish_parts(token) {
            if !pending.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut pending), base_style));
            }
            if !leading.is_empty() {
                spans.push(Span::styled(leading.to_string(), base_style));
            }
            spans.push(Span::styled(core.to_string(), code_style()));
            if !trailing.is_empty() {
                spans.push(Span::styled(trailing.to_string(), base_style));
            }
            pending.push_str(trailing_ws);
        } else {
            pending.push_str(token);
            pending.push_str(trailing_ws);
        }
    }

    if !pending.is_empty() {
        spans.push(Span::styled(pending, base_style));
    }
}

fn split_trailing_whitespace(s: &str) -> (&str, &str) {
    let trailing_start = s
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(idx, c)| idx + c.len_utf8())
        .unwrap_or(0);
    s.split_at(trailing_start)
}

fn codeish_parts(token: &str) -> Option<(&str, &str, &str)> {
    let start = token
        .char_indices()
        .find(|(_, c)| is_codeish_boundary_char(*c) || *c == '/' || c.is_ascii_alphanumeric())
        .map(|(idx, _)| idx)?;
    let end = token
        .char_indices()
        .rev()
        .find(|(_, c)| !is_codeish_trailing_punctuation(*c))
        .map(|(idx, c)| idx + c.len_utf8())?;

    if start >= end {
        return None;
    }

    let (leading, rest) = token.split_at(start);
    let (core, trailing) = rest.split_at(end - start);

    if is_codeish_token(core) {
        Some((leading, core, trailing))
    } else {
        None
    }
}

fn is_codeish_boundary_char(c: char) -> bool {
    matches!(c, '~' | '.')
}

fn is_codeish_trailing_punctuation(c: char) -> bool {
    matches!(
        c,
        ',' | '.' | ':' | ';' | ')' | ']' | '}' | '"' | '\'' | '，' | '。' | '）'
    )
}

fn is_codeish_token(token: &str) -> bool {
    if token.starts_with('/') && token[1..].contains('/') {
        return true;
    }

    if token.contains('/') && (token.contains('.') || token.starts_with("src/")) {
        return true;
    }

    [
        ".rs", ".ts", ".tsx", ".js", ".jsx", ".json", ".md", ".toml", ".yaml", ".yml",
    ]
    .iter()
    .any(|suffix| token.ends_with(suffix))
}

fn message_style() -> Style {
    Style::default().fg(Color::Gray)
}

fn prompt_bar_style() -> Style {
    Style::default()
        .fg(Color::Rgb(82, 86, 108))
        .bg(Color::Rgb(178, 183, 198))
        .add_modifier(Modifier::BOLD)
}

fn prompt_marker_style() -> Style {
    prompt_bar_style().fg(Color::Rgb(72, 77, 101))
}

fn thinking_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn quote_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn heading_style() -> Style {
    Style::default()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::BOLD)
}

fn strong_style(base_style: Style) -> Style {
    base_style.add_modifier(Modifier::BOLD)
}

fn gutter_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn marker_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn code_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
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

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn test_event_lines_preserve_raw_newlines_for_messages() {
        let lines = event_lines(&DisplayEvent::Message("intro\n- item".to_string()));

        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "• intro");
        assert_eq!(line_text(&lines[1]), "- item");
    }

    #[test]
    fn test_event_lines_highlight_inline_backtick_code() {
        let lines = event_lines(&DisplayEvent::Message(
            "Run `cargo test` before stopping".to_string(),
        ));

        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "• Run cargo test before stopping");
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "cargo test" && span.style == code_style()));
    }

    #[test]
    fn test_event_lines_preserve_bullet_structure() {
        let lines = event_lines(&DisplayEvent::Message(
            "Tasks:\n  - inspect\n  * verify".to_string(),
        ));

        assert_eq!(lines.len(), 3);
        assert_eq!(line_text(&lines[0]), "• Tasks:");
        assert_eq!(line_text(&lines[1]), "  - inspect");
        assert_eq!(line_text(&lines[2]), "  * verify");
    }

    #[test]
    fn test_event_lines_render_user_prompt_bar() {
        let lines = event_lines(&DisplayEvent::UserMessage("hello".to_string()));

        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "❯ hello");
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "❯ " && span.style == prompt_marker_style()));
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "hello" && span.style == prompt_bar_style()));
    }

    #[test]
    fn test_event_lines_render_multiline_user_prompt_with_continuation() {
        let lines = event_lines(&DisplayEvent::UserMessage("hello\nworld".to_string()));

        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "❯ hello");
        assert_eq!(line_text(&lines[1]), "  world");
    }

    #[test]
    fn test_events_lines_separate_user_and_assistant_turns() {
        let lines = events_lines(&[
            DisplayEvent::UserMessage("hello".to_string()),
            DisplayEvent::Message("hi".to_string()),
            DisplayEvent::UserMessage("again".to_string()),
        ]);
        let rendered: Vec<String> = lines.iter().map(line_text).collect();

        assert_eq!(rendered, vec!["❯ hello", "", "• hi", "", "❯ again"]);
    }

    #[test]
    fn test_events_lines_do_not_add_spacing_around_tool_summaries() {
        let lines = events_lines(&[
            DisplayEvent::Message("checking".to_string()),
            DisplayEvent::ToolCall {
                title: "Read file".to_string(),
                kind: "read".to_string(),
            },
            DisplayEvent::Message("done".to_string()),
        ]);
        let rendered: Vec<String> = lines.iter().map(line_text).collect();

        assert_eq!(rendered, vec!["• checking", "🔧 read: Read file", "• done"]);
    }

    #[test]
    fn test_event_lines_render_assistant_plain_paragraph_with_bullet() {
        let lines = event_lines(&DisplayEvent::Message("There's an issue".to_string()));

        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "• There's an issue");
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "• " && span.style == marker_style()));
    }

    #[test]
    fn test_event_lines_do_not_bullet_blank_lines() {
        let lines = event_lines(&DisplayEvent::Message("intro\n\nnext".to_string()));
        let rendered: Vec<String> = lines.iter().map(line_text).collect();

        assert_eq!(rendered, vec!["• intro", "", "• next"]);
    }

    #[test]
    fn test_event_lines_continue_plain_paragraph_without_extra_bullets() {
        let lines = event_lines(&DisplayEvent::Message(
            "first line\nsecond line".to_string(),
        ));
        let rendered: Vec<String> = lines.iter().map(line_text).collect();

        assert_eq!(rendered, vec!["• first line", "  second line"]);
    }

    #[test]
    fn test_event_lines_preserve_ordered_list_markers_without_extra_bullets() {
        let lines = event_lines(&DisplayEvent::Message("1. first\n2. second".to_string()));
        let rendered: Vec<String> = lines.iter().map(line_text).collect();

        assert_eq!(rendered, vec!["1. first", "2. second"]);
    }

    #[test]
    fn test_event_lines_do_not_duplicate_assistant_block_markers() {
        let lines = event_lines(&DisplayEvent::Message(
            "### Title\n- item\n> quote\n| File | Line |\n|---|---|\n| src/app.rs | 1 |\n```\ncode\n```"
                .to_string(),
        ));
        let rendered: Vec<String> = lines.iter().map(line_text).collect();

        assert_eq!(rendered[0], "### Title");
        assert_eq!(rendered[1], "- item");
        assert_eq!(rendered[2], "│ quote");
        assert_eq!(rendered[3], "File │ Line");
        assert_eq!(rendered[4], "src/app.rs │ 1");
        assert_eq!(rendered[5], "code");
        assert!(rendered.iter().all(|line| !line.starts_with("• #")));
        assert!(rendered.iter().all(|line| !line.starts_with("• -")));
        assert!(rendered.iter().all(|line| !line.starts_with("• │")));
    }

    #[test]
    fn test_event_lines_render_quotes_with_marker() {
        let lines = event_lines(&DisplayEvent::Message("> quoted `value`".to_string()));

        assert_eq!(line_text(&lines[0]), "│ quoted value");
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "│ " && span.style == marker_style()));
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "value" && span.style == code_style()));
    }

    #[test]
    fn test_event_lines_keep_unclosed_fenced_code_visible() {
        let lines = event_lines(&DisplayEvent::Message(
            "```\nlet x = 1;\nlet y = 2;".to_string(),
        ));

        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "let x = 1;");
        assert_eq!(line_text(&lines[1]), "let y = 2;");
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "let x = 1;" && span.style == code_style()));
    }

    #[test]
    fn test_event_lines_keep_unmatched_backtick_visible() {
        let lines = event_lines(&DisplayEvent::Message("Run `cargo test".to_string()));

        assert_eq!(line_text(&lines[0]), "• Run `cargo test");
    }

    #[test]
    fn test_event_lines_keep_tool_calls_compact() {
        let lines = event_lines(&DisplayEvent::ToolCall {
            title: "Read a very long generated implementation plan for events rendering behavior"
                .to_string(),
            kind: "read".to_string(),
        });

        assert_eq!(lines.len(), 1);
        assert_eq!(
            line_text(&lines[0]),
            "🔧 read: Read a very long generated implementation plan for..."
        );
    }

    #[test]
    fn test_event_lines_render_headings_without_hash_prefixes() {
        let lines = event_lines(&DisplayEvent::Message("### 调用位置".to_string()));

        assert_eq!(line_text(&lines[0]), "### 调用位置");
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "### " && span.style == marker_style()));
        assert!(lines[0]
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "调用位置" && span.style == heading_style()));
    }

    #[test]
    fn test_event_lines_render_bold_without_markers() {
        let lines = event_lines(&DisplayEvent::Message("- **请求方式**: POST".to_string()));

        assert_eq!(line_text(&lines[0]), "- 请求方式: POST");
        assert!(lines[0].spans.iter().any(|span| {
            span.content.as_ref() == "请求方式" && span.style == strong_style(message_style())
        }));
    }

    #[test]
    fn test_event_lines_render_simple_tables_without_separator_noise() {
        let lines = event_lines(&DisplayEvent::Message(
            "| 文件 | 行号 |\n|------|------|\n| src/pages/waybill/create/store.ts | 921 |"
                .to_string(),
        ));

        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "文件 │ 行号");
        assert_eq!(
            line_text(&lines[1]),
            "src/pages/waybill/create/store.ts │ 921"
        );
        assert!(lines[1].spans.iter().any(|span| span.content.as_ref()
            == "src/pages/waybill/create/store.ts"
            && span.style == code_style()));
    }

    #[test]
    fn test_event_lines_highlight_bare_api_paths() {
        let lines = event_lines(&DisplayEvent::Message(
            "接口 /yzg-saas-trans-app/yzgApp/supplement/create 的使用方式".to_string(),
        ));

        assert!(lines[0].spans.iter().any(|span| span.content.as_ref()
            == "/yzg-saas-trans-app/yzgApp/supplement/create"
            && span.style == code_style()));
    }

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
