# Claude-Like Prompt Composer Design

## Purpose

Upgrade the bottom Prompt composer from a simple append-only buffer into a Claude-like multiline editor for sending prompts into the selected acpx session.

The target interaction model is Claude/macOS plus readline compatibility, with Ghostty and iTerm2 as the primary terminals. The scope is limited to the Prompt composer editing experience. Slash commands, file mentions, and mode-switching command menus are out of scope for this phase.

## Approach

Use an existing TUI textarea component instead of continuing to grow a custom text editor. The selected dependency is `tui-textarea`, which provides textarea editing and ratatui integration.

The dependency should be wrapped behind project-owned modules so `tui-textarea` details do not spread across the application:

- `src/prompt_editor.rs` owns editor behavior and exposes acpx-tui-specific operations.
- `src/prompt_history.rs` owns session-scoped prompt history persistence.
- `src/app.rs` owns composer state, send lifecycle, focus, and history cursor state.
- `src/main.rs` maps terminal key events into app/editor actions.
- `src/ui.rs` renders the composer and short key hints.

## Editor Responsibilities

`PromptEditor` should wrap `tui-textarea` and expose a small API shaped around acpx-tui needs:

- Insert printable input and pasted text at the cursor.
- Insert newline for Shift+Enter-compatible paths.
- Delete before and after the cursor.
- Delete to current line start and current line end.
- Delete the previous word.
- Move left, right, up, down, to line start, and to line end.
- Report whether the cursor is at the buffer start or buffer end.
- Return the current text, replace it, and clear it.

Scrolling should follow the cursor. The previous user-controlled `prompt_scroll` behavior should be removed or reduced to internal editor viewport state.

## Key Semantics

The Prompt composer should support these keys:

| Key | Behavior |
| --- | --- |
| Printable input / paste | Insert at cursor |
| `Enter` | Send prompt |
| `Shift+Enter` | Insert newline |
| `Ctrl+J` | Insert newline, for terminals that emit LF for Shift+Enter |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character after cursor |
| `Left` / `Right` | Move cursor horizontally |
| `Up` / `Down` | Move cursor vertically, or navigate history at top/bottom boundary |
| `Ctrl+A` | Move to current line start |
| `Ctrl+E` | Move to current line end |
| `Cmd+Backspace` | Delete to current line start when the terminal delivers it |
| `Option+Backspace` | Delete previous word when the terminal delivers it |
| `Ctrl+W` | Delete previous word fallback |
| `Ctrl+U` | Delete to buffer start fallback |
| `Ctrl+K` | Delete to current line end fallback |
| `Esc` | Clear draft and leave Prompt focus |
| `Ctrl+C` | Clear draft; quit only when draft is already empty |

Command and Option key combinations are best-effort because terminal delivery differs by configuration. The stable fallback path is the readline-compatible `Ctrl` bindings.

## History Behavior

Prompt history is persistent and scoped by acpx session. This avoids mixing prompts across unrelated conversations.

History storage path:

```text
~/.acpx/tui/prompt-history.json
```

History format:

```json
{
  "version": 1,
  "sessions": {
    "agent:codex:acp:abc": ["first prompt", "second prompt"],
    "acp-session-id-fallback": ["other prompt"]
  }
}
```

The session key should use `acpx_control::prompt_session_selector(session)`, matching the actual send target. Named sessions are preferred; `acp_session_id` is the fallback.

Rules:

- Record a prompt when it is submitted for sending.
- Do not record empty or whitespace-only prompts.
- Do not append a prompt if it duplicates the most recent entry for that session.
- Keep at most 100 entries per session and drop the oldest entries first.
- Preserve the current unsent draft while navigating history.
- `Up` at the editor start selects older history.
- `Down` at the editor end selects newer history or restores the draft.
- Read/write failures must not block sending. Fall back to in-memory history and show a concise status when useful.
- If the JSON file is corrupt, rename it to `prompt-history.json.corrupt-<timestamp>` and start with an empty history.

## Error Handling

Editor operations should be total and non-panicking for normal key input. Unsupported terminal key events should be ignored or passed through to textarea defaults only when that does not conflict with the desired semantics.

History persistence should degrade gracefully. A bad or unwritable history file must not prevent typing or sending prompts.

Sending remains asynchronous through the existing prompt send channel. The composer should keep focus after send starts. The submitted text is cleared only after the send is accepted into the in-flight path.

## Testing

Add focused tests for:

- `PromptEditor`: insertion, newline insertion, character deletion, forward deletion, word deletion, delete-to-line-start, delete-to-line-end, cursor movement, and boundary checks.
- Key mapping: Enter sends, Shift+Enter and Ctrl+J insert newline, plain `j` remains text, and Ctrl+A/E/U/K/W map correctly.
- `PromptHistory`: session isolation, duplicate suppression, max length truncation, draft restoration, corrupt-file backup, and persistence round trip.
- `App`: submitted prompts are recorded in the selected session history, history navigation does not cross sessions, and failed history persistence does not block prompt submission.

Verification commands:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Manual verification should cover Ghostty and iTerm2 for Shift+Enter, Ctrl+J fallback, Option+Backspace, Command+Backspace, and readline fallback bindings.

## Migration Notes

Replace `prompt_buffer: String` with `PromptEditor`. Remove direct prompt-buffer mutation methods or turn them into thin wrappers around editor operations.

Replace `prompt_scroll` as a public app-level control with editor-managed viewport behavior. Update the composer hint text to mention the stable readline fallbacks where space allows.

Add only the `tui-textarea` dependency for this phase. Do not add slash command, mention, or command palette dependencies.
