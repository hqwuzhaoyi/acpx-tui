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
│ [Enter] Resume  [i/s] Send  [d] Details  [r] Refresh  [Ctrl+C] Quit │
└─────────────────────────────────────────────────────┘
```

## What it does

- Lists all acpx sessions from `~/.acpx/sessions/`
- Shows real-time ACP event stream (tool calls, user/agent messages, thinking, usage)
- Creates a fresh empty acpx session from a picked directory + registered agent
- Sends prompts back into the selected acpx session without leaving the TUI
- One-key resume: press Enter to `exec claude --resume <session_id>` into the full agent TUI

## Install

```bash
cargo install --path .
```

## Usage

```bash
acpx-tui
```

If you are developing locally, run without installing:

```bash
cargo run
```

### Keys

| Key | Action |
|-----|--------|
| `j/k` or `↑/↓` | Navigate sessions |
| `Enter` | Resume selected session (replaces current terminal) |
| `n` | Open the new-session launcher |
| `Tab` | Cycle focus: Sessions → Events → bottom Prompt composer |
| `i` or `s` | Jump focus directly to the bottom Prompt composer |
| Type/paste in Prompt composer | Edit a multiline prompt, Claude-style, at the bottom of the TUI |
| `←/→` in Prompt composer | Move the prompt cursor |
| `↑/↓` in Prompt composer | Move within multiline text; at the start/end, navigate that session's prompt history |
| `Ctrl+C` in Prompt composer | Clear current prompt; if already empty, quit the TUI |
| `Enter` in Prompt composer | Send prompt via `acpx <agent> prompt [-s <name>] <prompt>` |
| `Shift+Enter` or `Ctrl+J` in Prompt composer | Insert a newline without sending |
| `Ctrl+A/E` in Prompt composer | Move to current line start/end |
| `Ctrl+U/K` in Prompt composer | Delete to buffer start / current line end |
| `Ctrl+W` in Prompt composer | Delete previous word |
| `Cmd+Backspace` / `Option+Backspace` | Best-effort macOS delete-to-line-start / delete-word, when delivered by the terminal |
| `Esc` in Prompt composer | Clear prompt and return focus to Events |
| `d` | Toggle details view |
| `r` | Refresh session list |
| `Ctrl+C` outside Prompt composer | Quit |

### New-session launcher

Press `n` to create a fresh empty acpx session without leaving the TUI:

1. Type to fuzzy-filter directories, then press `Enter`.
2. Type to fuzzy-filter agents registered by `acpx`, then press `Enter`.
3. acpx-tui runs:

   ```bash
   acpx --cwd <selected-dir> <agent> sessions new
   ```

4. The session list refreshes so the new session is visible/reachable.

The directory picker is built into the ratatui UI and does not require an external `fzf` binary. It uses gitignore-aware directory discovery plus fuzzy ranking. The agent picker shows agents that `acpx` recognizes from its built-in registry merged with local config; it does not pre-launch health-check every agent. If session creation fails because the agent binary is missing, not authenticated, or otherwise unavailable, the acpx error is shown in the status bar and the TUI stays open.

## How it works

acpx-tui reads acpx's session storage at `~/.acpx/sessions/`:

- `index.json` → session list
- `<id>.json` → session metadata (agent type, cwd, pid, status)
- `<id>.stream.ndjson` → ACP JSON-RPC event stream

When you press Enter on a session, acpx-tui execs the agent's resume command (e.g., `claude --resume <acp_session_id>`), replacing itself with the full agent TUI. Your conversation history is preserved.

The new-session launcher discovers agent names by parsing the registered agent commands in `acpx --help`. It creates empty sessions through acpx's session command using the selected working directory. It intentionally does not send an initial prompt; use the bottom Prompt composer after creation if you want to send work into a session.

The bottom Prompt composer is always visible. Press `Tab` until it is focused, or press `i` / `s` to jump there directly, then type like a Claude Code-style multiline input box: `Enter` sends, `Shift+Enter` inserts a newline, and `Ctrl+C` clears the current prompt. If the composer is already empty, `Ctrl+C` quits the TUI; outside the composer, `Ctrl+C` also quits. The composer supports cursor movement, word/line deletion, and per-session prompt history. `↑/↓` move inside multiline text and switch history only at the start/end boundary. Prompt history is stored per target session in `~/.acpx/tui/prompt-history.json`. Ghostty and iTerm2 are the primary terminals; `Cmd`/`Option` key combinations are best-effort, with `Ctrl+A/E/U/K/W` as stable readline fallbacks. Sending uses acpx's named-session prompt command when the session has a name. Unnamed sessions are sent through the cwd-default acpx session by running in that session's cwd without `-s`. The TUI keeps polling the stream file, so user messages and agent responses appear in the Events panel after the send completes.

## Test with acpx + Codex

Use a named acpx session so both the CLI and TUI target the same conversation.

```bash
mkdir -p /tmp/acpx-codex-test
cd /tmp/acpx-codex-test

# Create or reuse a named Codex session for this cwd.
/opt/homebrew/bin/acpx codex sessions ensure --name tui-bridge-test

# Send the first prompt into the persistent session.
/opt/homebrew/bin/acpx codex prompt -s tui-bridge-test \
  "请记住：这个测试会话的代号是 tui-bridge-test。然后只回复一句：已记住。"

# Verify that follow-up prompts continue the same session.
/opt/homebrew/bin/acpx codex prompt -s tui-bridge-test \
  "刚才我让你记住的测试会话代号是什么？"
```

Then open the TUI:

```bash
cd /Users/admin/workspace/acpx-tui
cargo run
```

In the TUI:

1. Select the `codex` session for `/tmp/acpx-codex-test`.
2. Press `d` and confirm `Name` / `Prompt target` is `tui-bridge-test`.
3. Press `Tab` until the bottom Prompt composer is focused, or press `i` / `s` to jump there.
4. Type a follow-up prompt, then press `Enter`.
5. Watch the Events panel for `👤` user messages and `💬` agent replies.

If the session does not appear, press `r` to refresh or check that acpx wrote it:

```bash
grep -n "tui-bridge-test" ~/.acpx/sessions/index.json
```

If `acpx-tui` cannot find `acpx`, either ensure it is on `PATH` or set:

```bash
export ACPX_BIN=/opt/homebrew/bin/acpx
```

## Requirements

- [acpx](https://github.com/openclaw/acpx) (`npm install -g acpx`)
- An ACP-compatible agent (Claude Code, Codex, etc.)
