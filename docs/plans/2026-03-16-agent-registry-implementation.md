# Agent Registry Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extend acpx-tui from 2 agents (claude/codex) to 15 agents (all acpx built-ins + trae-cli) using a centralized Agent Registry.

**Architecture:** Add a new `src/agents.rs` module containing a static registry of all 15 agent definitions (name, color, resume pattern). Refactor `sessions.rs`, `resume.rs`, `ui.rs`, and `app.rs` to query this registry instead of using hardcoded match branches.

**Tech Stack:** Rust, ratatui (Color), existing crate dependencies (no new deps)

---

### Task 1: Create `src/agents.rs` with Agent Registry

**Files:**
- Create: `src/agents.rs`
- Modify: `src/main.rs:1` (add `mod agents;`)

**Step 1: Write the failing test**

Add to `src/agents.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_15_agents() {
        assert_eq!(AGENTS.len(), 15);
    }

    #[test]
    fn test_lookup_claude() {
        let info = lookup("claude").unwrap();
        assert_eq!(info.name, "claude");
        assert!(matches!(info.resume, ResumePattern::CliFlag { binary: "claude", .. }));
    }

    #[test]
    fn test_lookup_codex() {
        let info = lookup("codex").unwrap();
        assert_eq!(info.name, "codex");
        assert!(matches!(info.resume, ResumePattern::CliFlag { binary: "codex", .. }));
    }

    #[test]
    fn test_lookup_trae() {
        let info = lookup("trae").unwrap();
        assert_eq!(info.name, "trae");
        assert!(matches!(info.resume, ResumePattern::CliFlag { binary: "trae-cli", .. }));
    }

    #[test]
    fn test_lookup_unsupported_agent_has_unsupported_resume() {
        let info = lookup("gemini").unwrap();
        assert!(matches!(info.resume, ResumePattern::Unsupported));
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        assert!(lookup("nonexistent").is_none());
    }

    #[test]
    fn test_all_agents_have_unique_names() {
        let mut names: Vec<&str> = AGENTS.iter().map(|a| a.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), AGENTS.len());
    }

    #[test]
    fn test_resume_agents_count() {
        let resumable: Vec<_> = AGENTS.iter().filter(|a| matches!(a.resume, ResumePattern::CliFlag { .. })).collect();
        assert_eq!(resumable.len(), 3); // claude, codex, trae
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test agents::tests --lib`
Expected: FAIL — module doesn't exist yet

**Step 3: Write the implementation**

Create `src/agents.rs`:

```rust
use ratatui::style::Color;

/// Resume command pattern for an agent
pub enum ResumePattern {
    /// `<binary> <flag> <session_id>` — agent supports resume
    CliFlag {
        binary: &'static str,
        flag: &'static str,
    },
    /// Agent does not yet support resume
    Unsupported,
}

/// Metadata for a known agent
pub struct AgentInfo {
    pub name: &'static str,
    pub display_color: Color,
    pub resume: ResumePattern,
}

/// Static registry of all 15 supported agents
pub const AGENTS: &[AgentInfo] = &[
    AgentInfo { name: "pi",       display_color: Color::Green,            resume: ResumePattern::Unsupported },
    AgentInfo { name: "openclaw", display_color: Color::Blue,             resume: ResumePattern::Unsupported },
    AgentInfo { name: "codex",    display_color: Color::Cyan,             resume: ResumePattern::CliFlag { binary: "codex", flag: "resume" } },
    AgentInfo { name: "claude",   display_color: Color::Magenta,          resume: ResumePattern::CliFlag { binary: "claude", flag: "--resume" } },
    AgentInfo { name: "trae",     display_color: Color::LightCyan,        resume: ResumePattern::CliFlag { binary: "trae-cli", flag: "--resume" } },
    AgentInfo { name: "gemini",   display_color: Color::Yellow,           resume: ResumePattern::Unsupported },
    AgentInfo { name: "cursor",   display_color: Color::LightGreen,       resume: ResumePattern::Unsupported },
    AgentInfo { name: "copilot",  display_color: Color::White,            resume: ResumePattern::Unsupported },
    AgentInfo { name: "droid",    display_color: Color::LightRed,         resume: ResumePattern::Unsupported },
    AgentInfo { name: "iflow",    display_color: Color::LightBlue,        resume: ResumePattern::Unsupported },
    AgentInfo { name: "kilocode", display_color: Color::LightYellow,      resume: ResumePattern::Unsupported },
    AgentInfo { name: "kimi",     display_color: Color::LightMagenta,     resume: ResumePattern::Unsupported },
    AgentInfo { name: "kiro",     display_color: Color::Red,              resume: ResumePattern::Unsupported },
    AgentInfo { name: "opencode", display_color: Color::Gray,             resume: ResumePattern::Unsupported },
    AgentInfo { name: "qwen",     display_color: Color::Rgb(255, 165, 0), resume: ResumePattern::Unsupported },
];

/// Look up agent info by name
pub fn lookup(name: &str) -> Option<&'static AgentInfo> {
    AGENTS.iter().find(|a| a.name == name)
}
```

Add `mod agents;` to `src/main.rs` at line 1 (before other mod declarations).

**Step 4: Run tests to verify they pass**

Run: `cargo test agents::tests --lib`
Expected: all 7 tests PASS

**Step 5: Commit**

```bash
git add src/agents.rs src/main.rs
git commit -m "feat: add Agent Registry with 15 agent definitions"
```

---

### Task 2: Refactor `sessions.rs` — agent type parsing via registry

**Files:**
- Modify: `src/sessions.rs:79-95` (`parse_agent_type` function)
- Modify: `src/sessions.rs:1` (add `use crate::agents;`)

**Step 1: Write the failing tests**

Add to `src/sessions.rs` tests module (after existing tests):

```rust
#[test]
fn test_parse_agent_type_trae_cli() {
    assert_eq!(parse_agent_type("trae-cli acp serve"), "trae");
}

#[test]
fn test_parse_agent_type_trae_agent_alias() {
    assert_eq!(parse_agent_type("trae-agent --resume abc"), "trae");
}

#[test]
fn test_parse_agent_type_gemini() {
    assert_eq!(parse_agent_type("gemini --acp"), "gemini");
}

#[test]
fn test_parse_agent_type_cursor() {
    assert_eq!(parse_agent_type("cursor-agent acp"), "cursor");
}

#[test]
fn test_parse_agent_type_copilot() {
    assert_eq!(parse_agent_type("copilot --acp --stdio"), "copilot");
}

#[test]
fn test_parse_agent_type_kimi() {
    assert_eq!(parse_agent_type("kimi acp"), "kimi");
}

#[test]
fn test_parse_agent_type_kiro() {
    assert_eq!(parse_agent_type("kiro-cli acp"), "kiro");
}

#[test]
fn test_parse_agent_type_qwen() {
    assert_eq!(parse_agent_type("qwen --acp"), "qwen");
}

#[test]
fn test_parse_agent_type_droid() {
    assert_eq!(parse_agent_type("droid exec --output-format acp"), "droid");
}

#[test]
fn test_parse_agent_type_iflow() {
    assert_eq!(parse_agent_type("iflow --experimental-acp"), "iflow");
}

#[test]
fn test_parse_agent_type_kilocode() {
    assert_eq!(parse_agent_type("npx -y @kilocode/cli acp"), "kilocode");
}

#[test]
fn test_parse_agent_type_opencode() {
    assert_eq!(parse_agent_type("npx -y opencode-ai acp"), "opencode");
}

#[test]
fn test_parse_agent_type_pi() {
    assert_eq!(parse_agent_type("npx pi-acp"), "pi");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test sessions::tests --lib`
Expected: FAIL — new agent types not recognized

**Step 3: Implement the change**

Replace `src/sessions.rs:79-95` (`parse_agent_type`) with:

```rust
pub fn parse_agent_type(agent_command: &str) -> String {
    let cmd_lower = agent_command.to_lowercase();
    // Check all registered agents
    for agent in agents::AGENTS {
        if cmd_lower.contains(agent.name) {
            return agent.name.to_string();
        }
    }
    // Handle trae aliases (trae-cli, trae-agent) that contain "trae"
    if cmd_lower.contains("trae") {
        return "trae".to_string();
    }
    // Fallback: last token of the command
    agent_command
        .split_whitespace()
        .last()
        .unwrap_or("unknown")
        .to_string()
}
```

Add to top of `src/sessions.rs` (after line 1):

```rust
use crate::agents;
```

**Step 4: Run all tests**

Run: `cargo test sessions::tests --lib`
Expected: all tests PASS (existing + new)

**Step 5: Commit**

```bash
git add src/sessions.rs
git commit -m "refactor: parse_agent_type uses Agent Registry for all 15 agents"
```

---

### Task 3: Refactor `resume.rs` — table lookup instead of match

**Files:**
- Modify: `src/resume.rs:1-34` (imports + `build_resume_command`)

**Step 1: Write the failing test**

Add to `src/resume.rs` tests module:

```rust
#[test]
fn test_build_resume_command_trae() {
    let session = make_session("trae", "trae-sess-1");
    let (prog, args) = build_resume_command(&session).unwrap();
    assert_eq!(prog, "trae-cli");
    assert_eq!(args, vec!["--resume", "trae-sess-1"]);
}

#[test]
fn test_build_resume_command_gemini_unsupported() {
    let session = make_session("gemini", "gem-1");
    let result = build_resume_command(&session);
    assert!(result.is_err());
}

#[test]
fn test_build_resume_command_all_registered_unsupported() {
    // All unsupported agents should return UnsupportedAgent error
    for name in ["pi", "openclaw", "gemini", "cursor", "copilot", "droid", "iflow", "kilocode", "kimi", "kiro", "opencode", "qwen"] {
        let session = make_session(name, "sess-x");
        assert!(build_resume_command(&session).is_err(), "{} should be unsupported", name);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test resume::tests --lib`
Expected: FAIL — trae not in match, gemini returns `UnsupportedAgent` with wrong name

**Step 3: Implement the change**

Replace `src/resume.rs` imports and `build_resume_command` (lines 1-34) with:

```rust
use crate::agents::{self, ResumePattern};
use crate::sessions::Session;
use std::os::unix::process::CommandExt;
use std::process::Command;

#[derive(Debug)]
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

/// Build the resume command for a session by looking up the Agent Registry.
/// Returns (program, args) or error if agent doesn't support resume.
pub fn build_resume_command(session: &Session) -> Result<(String, Vec<String>), ResumeError> {
    let info = agents::lookup(&session.agent_type);
    match info.map(|i| &i.resume) {
        Some(ResumePattern::CliFlag { binary, flag }) => Ok((
            binary.to_string(),
            vec![flag.to_string(), session.acp_session_id.clone()],
        )),
        _ => Err(ResumeError::UnsupportedAgent(session.agent_type.clone())),
    }
}
```

**Step 4: Run all tests**

Run: `cargo test resume::tests --lib`
Expected: all tests PASS

Note: existing `test_build_resume_command_codex` expects `args == vec!["resume", "def-456"]` — the registry defines codex with `flag: "resume"` (not `--resume`), matching this expectation.

**Step 5: Commit**

```bash
git add src/resume.rs
git commit -m "refactor: build_resume_command uses Agent Registry lookup"
```

---

### Task 4: Update `ui.rs` — colored agent names

**Files:**
- Modify: `src/ui.rs:1-9` (add agents import)
- Modify: `src/ui.rs:63-69` (agent name rendering)

**Step 1: Write the failing test**

Add to `src/ui.rs` tests module:

```rust
#[test]
fn test_agent_color_lookup() {
    use crate::agents;
    use ratatui::style::Color;

    let claude = agents::lookup("claude").unwrap();
    assert_eq!(claude.display_color, Color::Magenta);

    let trae = agents::lookup("trae").unwrap();
    assert_eq!(trae.display_color, Color::LightCyan);

    let codex = agents::lookup("codex").unwrap();
    assert_eq!(codex.display_color, Color::Cyan);
}
```

**Step 2: Run test to verify it passes (lookup test is data-only)**

Run: `cargo test ui::tests::test_agent_color_lookup --lib`
Expected: PASS (this verifies the data is correct; the rendering change is visual)

**Step 3: Implement the colored agent name rendering**

Add import to `src/ui.rs` (after line 1):

```rust
use crate::agents;
```

Replace `src/ui.rs:63-69` (the two-line `line` construction inside `draw_sessions`) with:

```rust
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
```

**Step 4: Run all ui tests**

Run: `cargo test ui::tests --lib`
Expected: all tests PASS

**Step 5: Commit**

```bash
git add src/ui.rs
git commit -m "feat: colored agent names in session list via Agent Registry"
```

---

### Task 5: Update `app.rs` + `main.rs` — status message for unsupported resume

**Files:**
- Modify: `src/app.rs:4-11` (add `status_message` field)
- Modify: `src/app.rs` (add `set_status_message` and `clear_status_message` methods)
- Modify: `src/main.rs:43-60` (Enter key handler)
- Modify: `src/ui.rs:129-142` (status bar rendering)

**Step 1: Write the failing test**

Add to `src/app.rs` tests module:

```rust
#[test]
fn test_status_message() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::with_sessions_dir(dir.path());

    assert!(app.status_message.is_none());
    app.set_status_message("test message".to_string());
    assert_eq!(app.status_message.as_deref(), Some("test message"));
    app.clear_status_message();
    assert!(app.status_message.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test app::tests::test_status_message --lib`
Expected: FAIL — `status_message` field doesn't exist

**Step 3: Implement the changes**

Add `status_message` field to `App` struct in `src/app.rs:4-11`:

```rust
pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub events: Vec<DisplayEvent>,
    pub should_quit: bool,
    pub show_details: bool,
    pub status_message: Option<String>,
    sessions_dir: Option<std::path::PathBuf>,
}
```

Initialize `status_message: None` in both `new()` and `with_sessions_dir()`.

Add methods to `impl App`:

```rust
    pub fn set_status_message(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }
```

Update `src/main.rs:43-60` Enter key handler:

```rust
                    KeyCode::Enter => {
                        if let Some(session) = app.selected_session().cloned() {
                            let info = agents::lookup(&session.agent_type);
                            let can_resume = info
                                .map(|i| matches!(i.resume, agents::ResumePattern::CliFlag { .. }))
                                .unwrap_or(false);

                            if can_resume {
                                // Cleanup terminal before exec
                                disable_raw_mode()?;
                                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                terminal.show_cursor()?;

                                match resume::exec_resume(&session) {
                                    Err(e) => {
                                        eprintln!("{}", e);
                                        enable_raw_mode()?;
                                        execute!(io::stdout(), EnterAlternateScreen)?;
                                    }
                                    Ok(_) => unreachable!(),
                                }
                            } else {
                                app.set_status_message(
                                    format!("{} does not support resume yet", session.agent_type),
                                );
                            }
                        }
                    }
```

Add `use crate::agents;` import to `src/main.rs` (after line 7).

Update `draw_status_bar` in `src/ui.rs:129-142` to show status message:

```rust
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
```

Update `draw_status_bar` call site in `draw()` (`src/ui.rs:24`) to pass `app`:

```rust
    draw_status_bar(f, app, chunks[1]);
```

Update `draw_status_bar` signature (`src/ui.rs:129`):

```rust
fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
```

Clear status message on any key press (add to `src/main.rs`, at the top of the key handler, before the match):

```rust
                // Clear status message on any key press
                app.clear_status_message();
```

**Step 4: Run all tests**

Run: `cargo test --lib`
Expected: all tests PASS

**Step 5: Commit**

```bash
git add src/app.rs src/main.rs src/ui.rs
git commit -m "feat: show status message when resuming unsupported agent"
```

---

### Task 6: Build and verify

**Files:** None (verification only)

**Step 1: Run full test suite**

Run: `cargo test`
Expected: all tests PASS

**Step 2: Build release binary**

Run: `cargo build --release`
Expected: compiles without warnings

**Step 3: Verify binary runs**

Run: `cargo run --release` (then press `q` to quit)
Expected: TUI renders, shows sessions with colored agent names, quit works

**Step 4: Final commit (if any cleanup needed)**

```bash
git add -A
git commit -m "chore: verify build and test pass for agent registry"
```
