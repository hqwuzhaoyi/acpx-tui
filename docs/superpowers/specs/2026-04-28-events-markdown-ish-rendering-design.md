# Events Markdown-ish Rendering Design

## Problem

The Events panel now preserves long conversational content, but message bodies still render as mostly plain text. Structured agent output such as bullets, code snippets, quoted notes, commands, paths, and API routes is harder to scan than it needs to be in a TUI.

## Goal

Improve Events message readability with lightweight Markdown-ish rendering that resembles common agent transcript formatting without introducing a full Markdown renderer or new dependencies.

## Scope

In scope:

- Render message-like events (`Message`, `UserMessage`, `Thinking`) into styled `ratatui::Line` / `Span` values.
- Preserve existing wrapping and vertical scrolling behavior.
- Highlight inline backtick code.
- Highlight commands, paths, and API routes only when they are backtick-delimited in this iteration.
- Preserve and style simple bullet list structure.
- Render fenced code blocks as indented/styled text.
- Render simple block quotes with a visible quote marker and muted text.
- Keep tool calls and usage events compact.
- Add focused tests for the rendering helper behavior.

Out of scope:

- Full CommonMark compliance.
- Tables, link navigation, images, HTML, task lists, or nested Markdown block parsing.
- New dependencies.
- New keybindings or expand/collapse state.
- Changes to stream parsing or event merging.
- Card-style or expanded tool call rendering like the top half of the reference screenshot.
- Heuristic highlighting for bare paths, bare API routes, or bare commands outside backticks.

## Design

Add a small Markdown-ish rendering layer for the Events panel. Instead of converting every event with `format!("{}", e)` and wrapping it into one plain `Line`, the UI will ask a helper to convert each event into one or more styled lines.

Message-like events keep their existing icon prefix (`💬`, `👤`, `💭`) on the first rendered line. Continuation lines are indented so wrapped structures stay visually attached to the event. Tool calls and usage summaries keep their current compact one-line formatting.

Message-like rendering must use the raw string stored in `DisplayEvent`, preserving embedded newlines. It must not render from `format!("{}", event)` or other flattened display strings. The existing `Display` implementation remains useful for compact/plain fallback rendering and non-message summaries.

Supported lightweight structures:

- Inline code: text between backticks is rendered with an accent style.
- Bullets: lines starting with optional whitespace followed by `- ` or `* ` keep their indentation and use muted punctuation with readable body text.
- Fenced code blocks: lines between triple backticks render as code-style lines with indentation and accent coloring.
- Block quotes: lines starting with `>` render with a quote marker and muted text.
- Plain text: rendered as normal message text.

The implementation should favor readable output and graceful fallback over strict parsing. Malformed Markdown-like input remains visible as plain text.

## Rendering Rules

- Leading blank lines in message-like events are skipped.
- The first visible rendered line uses the event icon gutter, for example `💬 `.
- Later lines use a continuation gutter with the same display width as the icon gutter, for example `   `.
- Block renderers receive the current gutter; a bullet, quote, or code line never drops the event association.
- Bullet indentation is preserved after the gutter. Bullet punctuation is muted; bullet body text remains readable.
- Fenced code content remains visible even if the closing fence is missing.
- Incomplete inline code, such as an unmatched backtick, stays visible as plain text.

## Style Contract

Use existing TUI palette conventions:

- Normal message text: default or gray foreground.
- Inline code / backtick-delimited commands, paths, and API routes: cyan accent.
- Bullet markers and quote markers: dark gray or muted foreground.
- Code block content: light cyan or cyan accent.
- Thinking text may remain slightly muted compared with assistant/user messages.

## Components

- `src/events.rs`
  - Continue owning `DisplayEvent` and compact display formatting.
  - Keep long message text untruncated.

- `src/ui.rs`
  - Add a focused helper such as `event_lines(event: &DisplayEvent) -> Vec<Line<'static>>`.
  - Use the helper in `draw_events`.
  - Keep existing `Paragraph::wrap` and `.scroll((app.event_scroll, 0))`.

- Optional renderer module
  - Prefer a small private helper in `src/ui.rs`.
  - If the renderer grows beyond simple helpers, move it to a focused module such as `src/event_render.rs`.

## Data Flow

1. Stream events parse and merge into `DisplayEvent` as they do today.
2. `draw_events` converts each `DisplayEvent` into styled lines.
3. The Events paragraph wraps and scrolls those lines using existing ratatui behavior.

## Error Handling

No new runtime errors are expected. The renderer should treat incomplete inline code, unclosed fences, or unfamiliar Markdown constructs as visible plain text rather than dropping content.

## Testing

Run:

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`

Regression tests should cover:

- Inline backtick content becomes a distinct styled span.
- Bullet lines are preserved as separate rendered lines.
- Fenced code block content is not dropped.
- Quote lines retain visible quote structure.
- Tool calls still render as compact, truncated summaries.
- Raw newlines in message-like events produce multiple rendered lines.
- Rendering message-like events does not flatten through `DisplayEvent::Display`.
- Unmatched inline backticks remain visible.
- Unclosed fenced code blocks keep their content visible.

## Risks

This increases UI rendering complexity in `src/ui.rs`. Keep the parser deliberately small and local so it remains easy to reason about and can be replaced later if a full renderer is ever justified.
