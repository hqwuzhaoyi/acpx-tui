# CLAUDE.md

## Project

acpx-tui: TUI dashboard for [acpx](https://github.com/openclaw/acpx) sessions with one-click resume.

**Tech stack:** Rust, ratatui 0.29, crossterm 0.28

## Architecture

```
src/
  main.rs       - CLI entry + TUI event loop (crossterm + ratatui)
  app.rs        - App state: session list, selection, events, quit/details flags
  sessions.rs   - Read ~/.acpx/sessions/ (index.json + <id>.json)
  events.rs     - Parse .stream.ndjson ACP JSON-RPC events
  ui.rs         - Two-panel layout rendering (sessions + events)
  resume.rs     - exec() into agent CLI to resume session
```

## Data source

- `~/.acpx/sessions/index.json` — session list (camelCase fields)
- `~/.acpx/sessions/<id>.json` — session detail (snake_case fields)
- `~/.acpx/sessions/<id>.stream.ndjson` — ACP JSON-RPC event stream

## Key findings

### Resume requires correct cwd

`claude --resume <session_id>` resolves sessions by project directory. Claude Code stores sessions under `~/.claude/projects/<encoded-cwd>/`. If you run `claude --resume` from a different directory than the session's original `cwd`, it fails with "No conversation found". Fix: `chdir` to `session.cwd` before `exec`.

### Agent resume command formats differ

- **Claude Code:** `claude --resume <session_id>` (flag)
- **Codex CLI:** `codex resume <session_id>` (subcommand)

Each agent has its own CLI convention. When adding new agents, check their `--help` for the correct resume syntax.

### ACP event format (real data)

- `usage_update` has `used` and `size` fields (NOT `cost.amount`)
- `tool_call` has `title`, `kind`, `status` fields
- `tool_call_update` has `toolCallId`, `status` (no `title`)
- `agent_message_chunk` has `content.type` + `content.text`
- `agent_thought_chunk` same structure as message_chunk

## Commands

```bash
cargo test          # Run 46 unit tests
cargo run           # Launch TUI
cargo build --release  # Release binary (target/release/acpx-tui)
```

## Testing

Tests use `tempfile` crate for isolated test directories. Session and event parsing modules have full test coverage with fixtures based on real acpx data.
