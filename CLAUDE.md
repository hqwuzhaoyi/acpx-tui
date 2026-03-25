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

- **Claude Code:** `claude --resume <session_id>` (flag + separate arg)
- **Codex CLI:** `codex resume <session_id>` (subcommand + separate arg)
- **Trae CLI:** `trae-cli --resume=<session_id>` (flag=value, MUST use `=`)

Trae's `--resume` is `string[="AUTO"]` — without `=`, trae treats it as `--resume=AUTO` and the session ID becomes a prompt message. Use `CliFlagEq` pattern for agents with optional-value flags.

### acpx session architecture

acpx runs agents in ACP server mode (e.g. `trae-cli acp serve`) as background processes. Two independent session systems coexist:

- **acpx sessions** (`~/.acpx/sessions/`): managed by acpx, events recorded in `.stream.ndjson`
- **Agent-native sessions** (e.g. `~/Library/Caches/trae-cli/sessions/`): managed by the agent CLI itself

`trae-cli --resume` uses trae's own session system and does NOT update acpx's stream files. `acpx trae --session <name>` goes through the ACP proxy and records events properly. Both paths are independent — using `trae-cli --resume` does not interrupt acpx's session.

### Stream files may not exist

Sessions with `last_seq: 0` have no `.stream.ndjson` file — the file is only created when events flow through the ACP proxy. Sessions where `acpxRecordId == acpSessionId` never completed ACP handshake.

### Agent ACP capabilities vary

- **Claude/Codex:** support `loadSession` — acpx can resume ACP sessions
- **Trae:** no `loadSession` — acpx always creates new ACP session on reconnect (but trae-cli's own `--resume` works independently)

### index.json optional fields

`name` field is optional in index.json entries. Present when session was created with `acpx <agent> sessions new --name <name>`.

### ACP event format (real data)

- `usage_update` has `used` and `size` fields (NOT `cost.amount`)
- `tool_call` has `title`, `kind`, `status` fields
- `tool_call_update` has `toolCallId`, `status` (no `title`)
- `agent_message_chunk` has `content.type` + `content.text`
- `agent_thought_chunk` same structure as message_chunk

## Commands

```bash
cargo test          # Run 88 unit tests
cargo run           # Launch TUI
cargo build --release  # Release binary (target/release/acpx-tui)
```

## Testing

Tests use `tempfile` crate for isolated test directories. Session and event parsing modules have full test coverage with fixtures based on real acpx data.
