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

## Design

Add a small Markdown-ish rendering layer for the Events panel. Instead of converting every event with `format!("{}", e)` and wrapping it into one plain `Line`, the UI will ask a helper to convert each event into one or more styled lines.

Message-like events keep their existing icon prefix (`💬`, `👤`, `💭`) on the first rendered line. Continuation lines are indented so wrapped structures stay visually attached to the event. Tool calls and usage summaries keep their current compact one-line formatting.

Supported lightweight structures:

- Inline code: text between backticks is rendered with an accent style.
- Bullets: lines starting with optional whitespace followed by `- ` or `* ` keep their indentation and use muted punctuation with readable body text.
- Fenced code blocks: lines between triple backticks render as code-style lines with indentation and accent coloring.
- Block quotes: lines starting with `>` render with a quote marker and muted text.
- Plain text: rendered as normal message text.

The implementation should favor readable output and graceful fallback over strict parsing. Malformed Markdown-like input remains visible as plain text.

## Components

- `src/events.rs`
  - Continue owning `DisplayEvent` and compact display formatting.
  - Keep long message text untruncated.

- `src/ui.rs`
  - Add a focused helper such as `event_lines(event: &DisplayEvent) -> Vec<Line<'static>>`.
  - Use the helper in `draw_events`.
  - Keep existing `Paragraph::wrap` and `.scroll((app.event_scroll, 0))`.

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

## Risks

This increases UI rendering complexity in `src/ui.rs`. Keep the parser deliberately small and local so it remains easy to reason about and can be replaced later if a full renderer is ever justified.
