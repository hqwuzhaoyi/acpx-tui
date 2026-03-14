# acpx-tui

TUI dashboard for [acpx](https://github.com/openclaw/acpx) sessions with one-click resume.

```
┌─ acpx-tui ────────────────────────────────────────┐
│ Sessions                 │ Events                  │
│                          │                          │
│ ● claude  ~/project-a    │ 🔧 execute: cargo build  │
│   5m ago · running       │ 💬 "Done. PR ready."    │
│                          │                          │
│ ○ codex   ~/project-b   │ 🔧 execute: npm test     │
│   2h ago · exited        │ 💰 $0.44                │
│                          │                          │
├──────────────────────────┴──────────────────────────┤
│ [Enter] Resume  [d] Details  [r] Refresh  [q] Quit │
└─────────────────────────────────────────────────────┘
```

## What it does

- Lists all acpx sessions from `~/.acpx/sessions/`
- Shows real-time ACP event stream (tool calls, messages, thinking, cost)
- One-key resume: press Enter to `exec claude --resume <session_id>` into the full agent TUI

## Install

```bash
cargo install --path .
```

## Usage

```bash
acpx-tui
```

### Keys

| Key | Action |
|-----|--------|
| `j/k` or `↑/↓` | Navigate sessions |
| `Enter` | Resume selected session (replaces current terminal) |
| `d` | Toggle details view |
| `r` | Refresh session list |
| `q` | Quit |

## How it works

acpx-tui reads acpx's session storage at `~/.acpx/sessions/`:

- `index.json` → session list
- `<id>.json` → session metadata (agent type, cwd, pid, status)
- `<id>.stream.ndjson` → ACP JSON-RPC event stream

When you press Enter on a session, acpx-tui execs the agent's resume command (e.g., `claude --resume <acp_session_id>`), replacing itself with the full agent TUI. Your conversation history is preserved.

## Requirements

- [acpx](https://github.com/openclaw/acpx) (`npm install -g acpx`)
- An ACP-compatible agent (Claude Code, Codex, etc.)
