# acpx-tui Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a TUI dashboard that displays acpx sessions and lets users one-click resume into a full agent terminal.

**Architecture:** Read `~/.acpx/sessions/index.json` for session list, `<id>.json` for metadata, tail `<id>.stream.ndjson` for live events. Two-panel ratatui layout (sessions + events). Enter key execs `claude --resume` replacing the current process.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28, serde, clap 4

---

### Task 1: Scaffold project

**Repo:** https://github.com/hqwuzhaoyi/acpx-tui (already created)

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Already done: `.gitignore`, `README.md`, git init, GitHub repo

**Step 1: Write Cargo.toml**

```toml
[package]
name = "acpx-tui"
version = "0.1.0"
edition = "2021"
description = "TUI dashboard for acpx sessions with one-click resume"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
```

**Step 3: Write minimal main.rs**

```rust
fn main() {
    println!("acpx-tui");
}
```

**Step 4: Write .gitignore**

```
/target
```

**Step 5: Build to verify setup**

Run: `cd /Users/admin/workspace/acpx-tui && cargo build`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add Cargo.toml src/main.rs .gitignore
git commit -m "chore: scaffold acpx-tui project"
```

---

### Task 2: Session data types and reader (`sessions.rs`)

**Files:**
- Create: `src/sessions.rs`
- Modify: `src/main.rs`

**Step 1: Write sessions.rs with data types matching real acpx format**

The real `index.json` uses camelCase, the real `<id>.json` uses snake_case. Both need explicit serde rename.

```rust
use serde::Deserialize;
use std::path::PathBuf;

/// ~/.acpx/sessions/index.json
#[derive(Debug, Deserialize)]
pub struct SessionIndex {
    pub entries: Vec<SessionIndexEntry>,
}

/// One entry in index.json (camelCase)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexEntry {
    pub file: String,
    pub acpx_record_id: String,
    pub acp_session_id: String,
    pub agent_command: String,
    pub cwd: String,
    pub closed: bool,
    pub last_used_at: String,
}

/// Full session detail from <id>.json (snake_case)
#[derive(Debug, Deserialize)]
pub struct SessionDetail {
    pub acpx_record_id: String,
    pub acp_session_id: String,
    pub agent_command: String,
    pub cwd: String,
    pub created_at: String,
    pub last_used_at: String,
    pub closed: bool,
    pub pid: Option<u32>,
    pub agent_started_at: Option<String>,
    pub last_agent_exit_at: Option<String>,
    pub last_agent_disconnect_reason: Option<String>,
    pub event_log: Option<EventLog>,
}

#[derive(Debug, Deserialize)]
pub struct EventLog {
    pub active_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Running,
    Exited,
    Closed,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Running => write!(f, "running"),
            SessionStatus::Exited => write!(f, "exited"),
            SessionStatus::Closed => write!(f, "closed"),
        }
    }
}

/// Resolved session info for display
#[derive(Debug, Clone)]
pub struct Session {
    pub acpx_record_id: String,
    pub acp_session_id: String,
    pub agent_type: String,
    pub cwd: String,
    pub status: SessionStatus,
    pub last_used_at: String,
    pub stream_path: Option<String>,
}

fn sessions_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".acpx")
        .join("sessions")
}

/// Parse agent type from agent_command string
/// "npx -y @zed-industries/claude-agent-acp@^0.21.0" → "claude"
/// "npx @zed-industries/codex-acp@^0.9.5" → "codex"
fn parse_agent_type(agent_command: &str) -> String {
    if agent_command.contains("claude") {
        "claude".to_string()
    } else if agent_command.contains("codex") {
        "codex".to_string()
    } else if agent_command.contains("gemini") {
        "gemini".to_string()
    } else if agent_command.contains("openclaw") {
        "openclaw".to_string()
    } else {
        agent_command
            .split_whitespace()
            .last()
            .unwrap_or("unknown")
            .to_string()
    }
}

/// Check if pid is alive
fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Determine session status
fn resolve_status(detail: &SessionDetail) -> SessionStatus {
    if detail.closed {
        return SessionStatus::Closed;
    }
    if let Some(pid) = detail.pid {
        if is_pid_alive(pid) {
            return SessionStatus::Running;
        }
    }
    if detail.last_agent_exit_at.is_some() {
        return SessionStatus::Exited;
    }
    SessionStatus::Exited
}

/// Load all sessions from ~/.acpx/sessions/
pub fn load_sessions() -> Vec<Session> {
    let dir = sessions_dir();
    let index_path = dir.join("index.json");

    let data = match std::fs::read_to_string(&index_path) {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    let index: SessionIndex = match serde_json::from_str(&data) {
        Ok(i) => i,
        Err(_) => return vec![],
    };

    index
        .entries
        .iter()
        .filter_map(|entry| {
            let detail_path = dir.join(&entry.file);
            let detail_data = std::fs::read_to_string(&detail_path).ok()?;
            let detail: SessionDetail = serde_json::from_str(&detail_data).ok()?;
            let status = resolve_status(&detail);
            let stream_path = detail.event_log.map(|e| e.active_path);

            Some(Session {
                acpx_record_id: entry.acpx_record_id.clone(),
                acp_session_id: entry.acp_session_id.clone(),
                agent_type: parse_agent_type(&entry.agent_command),
                cwd: entry.cwd.clone(),
                status,
                last_used_at: entry.last_used_at.clone(),
                stream_path,
            })
        })
        .collect()
}
```

**Step 2: Add `libc` dependency to Cargo.toml**

Add under `[dependencies]`:
```toml
libc = "0.2"
dirs = "6"
```

**Step 3: Update main.rs to test loading**

```rust
mod sessions;

fn main() {
    let sessions = sessions::load_sessions();
    for s in &sessions {
        println!("{} {} {} {}", s.agent_type, s.acp_session_id, s.cwd, s.status);
    }
}
```

**Step 4: Run to verify**

Run: `cargo run`
Expected: Prints session list like `claude 4ed50f0f-... /Users/admin/workspace/code-agent-monitor exited`

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: add session data reader"
```

---

### Task 3: Event stream parser (`events.rs`)

**Files:**
- Create: `src/events.rs`

**Step 1: Write events.rs**

Parse NDJSON ACP events from `.stream.ndjson`. Only extract display-relevant events.

```rust
use serde::Deserialize;
use std::io::{BufRead, BufReader, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub enum DisplayEvent {
    Message(String),
    ToolCall { title: String, kind: String },
    Thinking(String),
    Usage { cost: f64 },
}

impl std::fmt::Display for DisplayEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisplayEvent::Message(text) => write!(f, "💬 {}", truncate(text, 60)),
            DisplayEvent::ToolCall { title, kind } => write!(f, "🔧 {}: {}", kind, truncate(title, 50)),
            DisplayEvent::Thinking(text) => write!(f, "💭 {}", truncate(text, 60)),
            DisplayEvent::Usage { cost } => write!(f, "💰 ${:.4}", cost),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s
    }
}

/// Raw JSON-RPC message shape (only fields we care about)
#[derive(Deserialize)]
struct RpcMessage {
    method: Option<String>,
    params: Option<serde_json::Value>,
}

/// Load last N events from a .stream.ndjson file
pub fn load_recent_events(path: &str, max_events: usize) -> Vec<DisplayEvent> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };

    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if let Some(event) = parse_event(&line) {
            events.push(event);
        }
    }

    // Return last N
    if events.len() > max_events {
        events.split_off(events.len() - max_events)
    } else {
        events
    }
}

fn parse_event(line: &str) -> Option<DisplayEvent> {
    let msg: RpcMessage = serde_json::from_str(line).ok()?;

    if msg.method.as_deref() != Some("session/update") {
        return None;
    }

    let params = msg.params?;
    let update = params.get("update")?;
    let session_update = update.get("sessionUpdate")?.as_str()?;

    match session_update {
        "agent_message_chunk" => {
            let text = update
                .get("content")?
                .get("text")?
                .as_str()?;
            if text.is_empty() {
                return None;
            }
            Some(DisplayEvent::Message(text.to_string()))
        }
        "tool_call_update" => {
            let title = update.get("title")?.as_str()?.to_string();
            let kind = update
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("tool")
                .to_string();
            Some(DisplayEvent::ToolCall { title, kind })
        }
        "agent_thought_chunk" => {
            let text = update
                .get("content")?
                .get("text")?
                .as_str()?;
            if text.len() < 10 {
                return None; // Skip tiny incremental chunks
            }
            Some(DisplayEvent::Thinking(text.to_string()))
        }
        "usage_update" => {
            let cost = update
                .get("cost")?
                .get("amount")?
                .as_f64()?;
            Some(DisplayEvent::Usage { cost })
        }
        _ => None,
    }
}
```

**Step 2: Test in main.rs**

```rust
mod sessions;
mod events;

fn main() {
    let sessions = sessions::load_sessions();
    for s in &sessions {
        println!("\n=== {} ({}) ===", s.agent_type, s.status);
        if let Some(ref path) = s.stream_path {
            let evts = events::load_recent_events(path, 10);
            for e in &evts {
                println!("  {}", e);
            }
        }
    }
}
```

**Step 3: Run to verify**

Run: `cargo run`
Expected: Prints sessions with their recent events (tool calls, messages, etc.)

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add NDJSON event stream parser"
```

---

### Task 4: Resume logic (`resume.rs`)

**Files:**
- Create: `src/resume.rs`

**Step 1: Write resume.rs**

```rust
use crate::sessions::Session;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub enum ResumeError {
    UnsupportedAgent(String),
}

impl std::fmt::Display for ResumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResumeError::UnsupportedAgent(agent) => {
                write!(f, "Resume not supported for agent: {}", agent)
            }
        }
    }
}

/// Build the resume command for a session.
/// Returns (program, args) or error.
fn build_resume_command(session: &Session) -> Result<(String, Vec<String>), ResumeError> {
    match session.agent_type.as_str() {
        "claude" => Ok((
            "claude".to_string(),
            vec![
                "--resume".to_string(),
                session.acp_session_id.clone(),
            ],
        )),
        "codex" => Ok((
            "codex".to_string(),
            vec![
                "--resume".to_string(),
                session.acp_session_id.clone(),
            ],
        )),
        other => Err(ResumeError::UnsupportedAgent(other.to_string())),
    }
}

/// Exec into the agent TUI, replacing the current process.
/// This function does not return on success.
pub fn exec_resume(session: &Session) -> Result<(), ResumeError> {
    let (program, args) = build_resume_command(session)?;

    let err = Command::new(&program)
        .args(&args)
        .exec();

    // exec() only returns on error
    eprintln!("Failed to exec {} --resume: {}", program, err);
    std::process::exit(1);
}
```

**Step 2: Commit**

```bash
git add src/resume.rs
git commit -m "feat: add resume exec logic"
```

---

### Task 5: App state (`app.rs`)

**Files:**
- Create: `src/app.rs`

**Step 1: Write app.rs**

```rust
use crate::events::{self, DisplayEvent};
use crate::sessions::{self, Session};

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub events: Vec<DisplayEvent>,
    pub should_quit: bool,
    pub show_details: bool,
}

impl App {
    pub fn new() -> Self {
        let sessions = sessions::load_sessions();
        let events = if let Some(s) = sessions.first() {
            load_events_for(s)
        } else {
            vec![]
        };

        App {
            sessions,
            selected: 0,
            events,
            should_quit: false,
            show_details: false,
        }
    }

    pub fn refresh(&mut self) {
        self.sessions = sessions::load_sessions();
        if self.selected >= self.sessions.len() && !self.sessions.is_empty() {
            self.selected = self.sessions.len() - 1;
        }
        self.reload_events();
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.sessions.get(self.selected)
    }

    pub fn select_next(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = (self.selected + 1).min(self.sessions.len() - 1);
            self.reload_events();
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.reload_events();
        }
    }

    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }

    fn reload_events(&mut self) {
        self.events = self
            .selected_session()
            .map(|s| load_events_for(s))
            .unwrap_or_default();
    }
}

fn load_events_for(session: &Session) -> Vec<DisplayEvent> {
    session
        .stream_path
        .as_ref()
        .map(|p| events::load_recent_events(p, 50))
        .unwrap_or_default()
}
```

**Step 2: Commit**

```bash
git add src/app.rs
git commit -m "feat: add app state management"
```

---

### Task 6: TUI rendering (`ui.rs`)

**Files:**
- Create: `src/ui.rs`

**Step 1: Write ui.rs**

```rust
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

            let line = Line::from(vec![
                Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
                Span::styled(format!("{:<8}", s.agent_type), style),
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
    let lines: Vec<Line> = app
        .events
        .iter()
        .map(|e| Line::from(format!("{}", e)))
        .collect();

    let title = if let Some(s) = app.selected_session() {
        format!(" Events [{}] ", s.acp_session_id.chars().take(8).collect::<String>())
    } else {
        " Events ".to_string()
    };

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, _app: &App, area: Rect) {
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

fn shorten_path(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Some(rest) = path.strip_prefix(home.to_str().unwrap_or("")) {
            return format!("~{}", rest);
        }
    }
    path.to_string()
}

fn format_age(iso: &str) -> String {
    // Simple age formatting from ISO timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Parse ISO 8601 timestamp manually (avoid chrono dependency)
    // Format: "2026-03-14T14:38:58.516Z"
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
    // Minimal ISO 8601 parser: "2026-03-14T14:38:58.516Z"
    let s = s.trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 { return None; }
    let year: u64 = parts[0].parse().ok()?;
    let month: u64 = parts[1].parse().ok()?;
    let day: u64 = parts[2].parse().ok()?;

    let time_parts: Vec<&str> = time.split('.').next()?.split(':').collect();
    if time_parts.len() != 3 { return None; }
    let hour: u64 = time_parts[0].parse().ok()?;
    let min: u64 = time_parts[1].parse().ok()?;
    let sec: u64 = time_parts[2].parse().ok()?;

    // Rough epoch calculation (not accounting for leap years precisely)
    let days = (year - 1970) * 365 + (year - 1969) / 4
        + match month {
            1 => 0, 2 => 31, 3 => 59, 4 => 90, 5 => 120, 6 => 151,
            7 => 181, 8 => 212, 9 => 243, 10 => 273, 11 => 304, 12 => 334,
            _ => 0,
        }
        + day - 1;

    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}
```

**Step 2: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add TUI rendering"
```

---

### Task 7: Wire everything together in main.rs

**Files:**
- Modify: `src/main.rs`

**Step 1: Write the full main.rs**

```rust
mod app;
mod events;
mod resume;
mod sessions;
mod ui;

use app::App;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::time::Duration;

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Auto-refresh timer
    let tick_rate = Duration::from_secs(3);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                    KeyCode::Char('r') => app.refresh(),
                    KeyCode::Char('d') => app.toggle_details(),
                    KeyCode::Enter => {
                        if let Some(session) = app.selected_session().cloned() {
                            // Cleanup terminal before exec
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                LeaveAlternateScreen
                            )?;
                            terminal.show_cursor()?;

                            match resume::exec_resume(&session) {
                                Err(e) => {
                                    eprintln!("{}", e);
                                    // Re-enter TUI on error
                                    enable_raw_mode()?;
                                    execute!(
                                        io::stdout(),
                                        EnterAlternateScreen
                                    )?;
                                }
                                Ok(_) => unreachable!(), // exec doesn't return
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else {
            // Tick: auto-refresh sessions
            app.refresh();
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
```

**Step 2: Build and test**

Run: `cargo build`
Expected: Compiles successfully

Run: `cargo run`
Expected: TUI appears showing acpx sessions with events, responds to j/k/Enter/q

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire TUI main loop with session list and resume"
```

---

### Task 8: Polish and edge cases

**Files:**
- Modify: `src/app.rs`
- Modify: `src/ui.rs`

**Step 1: Handle empty state in ui.rs**

When there are no sessions, show a helpful message instead of a blank screen. Add to `draw_sessions`:

```rust
if app.sessions.is_empty() {
    let msg = Paragraph::new("No acpx sessions found.\n\nStart one with: acpx claude \"your prompt\"")
        .block(Block::default().borders(Borders::ALL).title(" Sessions "))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(msg, area);
    return;
}
```

**Step 2: Add details view toggle**

When `d` is pressed, right panel shows raw session JSON instead of events. Add to `draw_events` top:

```rust
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
```

**Step 3: Build and test**

Run: `cargo run`
Expected: Empty state shows message, `d` toggles between events and details view

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add empty state and details view"
```

---

### Task 9: Build release and verify end-to-end

**Step 1: Build release binary**

Run: `cargo build --release`
Expected: Binary at `target/release/acpx-tui`

**Step 2: End-to-end test**

```bash
# Run the TUI
./target/release/acpx-tui

# Verify:
# 1. Sessions appear with correct agent type and status
# 2. j/k navigation works
# 3. Events panel shows recent events for selected session
# 4. d toggles details view
# 5. r refreshes session list
# 6. Enter on a claude session execs claude --resume
# 7. q exits cleanly
```

**Step 3: Final commit**

```bash
git add -A
git commit -m "chore: release build verification"
```
