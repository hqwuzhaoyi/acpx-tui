# Events Claude-like Role Display Design

## Problem

The Events panel can now render message content with Markdown-ish styling, but user prompts and assistant responses still feel too similar. This makes multi-turn conversations harder to scan than Claude Code-style transcripts, where user input appears as a distinct prompt bar and assistant output reads as response content.

## Goal

Make Events conversations visually distinguish user and AI content in a Claude Code-like way without changing event parsing, adding dependencies, or expanding tool-call rendering.

## Reference Behavior

Target visual direction:

```text
❯ 123

• There's an issue with the selected model (glm-4.7). It may not exist or you
  may not have access to it. Run /model to pick a different model.

❯ hello

• There's an issue with the selected model (glm-4.7). It may not exist or you
  may not have access to it. Run /model to pick a different model.
```

The exact colors should follow the existing TUI palette, but the structure should match:

- User input is a single prompt-style bar with a `❯` marker.
- Assistant output is normal transcript content, not another prompt bar.
- Assistant plain paragraphs may use a muted bullet marker for scanability.
- Wrapped assistant lines align under the paragraph body rather than under the bullet.

## Scope

In scope:

- Render `DisplayEvent::UserMessage` as a prompt bar-style line.
- Render `DisplayEvent::Message` as assistant transcript content using the existing Markdown-ish renderer.
- Give assistant plain paragraphs a bullet-style marker when they are not already block structures such as headings, bullets, quotes, tables, or code.
- Preserve raw newline handling, Markdown-ish styling, and vertical scrolling.
- Keep `Thinking`, `ToolCall`, and `Usage` compact and visually distinct from user prompts.
- Add tests for user prompt bars and assistant paragraph markers.

Out of scope:

- Full Claude Code theme replication.
- Tool-call cards or expanded tool details.
- Timestamps, avatars, or model names.
- New keybindings or navigation behavior.
- Changes to event parsing or event merging.

## Design

Extend the current `event_lines(event: &DisplayEvent)` renderer:

- `UserMessage`
  - Render as a single prompt bar when the user content is one line.
  - For multi-line user input, render the first line with `❯ ` and continuation lines with a matching gutter.
  - Use a muted background or strong contrast style available in ratatui so the line reads like an input prompt.
  - Keep inline code/path highlighting where practical, but user prompt identity is more important than Markdown richness.

- `Message`
  - Continue using raw text and Markdown-ish rendering.
  - Plain paragraph lines get a leading `• ` marker.
  - Existing structures keep their own markers:
    - Headings do not get an extra bullet.
    - Bullets do not get an extra bullet.
    - Quotes keep quote markers.
    - Table rows keep table formatting.
    - Code blocks keep code styling.

- `Thinking`
  - Continue using the Markdown-ish renderer with muted styling, but do not make it look like user input.

- `ToolCall` / `Usage`
  - Continue compact one-line rendering.

## Rendering Rules

- User prompt gutter is `❯ ` on the first visible line.
- User continuation gutter has the same visual width as the prompt gutter.
- Assistant plain paragraph gutter is `• ` on the first visual line and a two-space continuation gutter for additional raw lines in the same paragraph.
- Empty lines between user and assistant messages are acceptable if they improve scanability.
- Existing event scroll preservation must continue to work.

## Style Contract

- User prompt bar: muted gray background with readable foreground, following the reference screenshot.
- User prompt marker: slightly stronger than the prompt text.
- Assistant bullet marker: muted foreground.
- Assistant content: existing message style, with inline code/path accents preserved.
- Thinking content: muted compared with normal assistant content.

## Components

- `src/ui.rs`
  - Adjust `event_lines` and Markdown-ish helpers to distinguish user prompt rendering from assistant rendering.
  - Keep renderer dependency-free and local unless complexity requires a focused module.

- `src/events.rs`
  - No planned semantic changes.

- `src/app.rs`
  - No planned changes beyond preserving existing scroll behavior.

## Testing

Run:

- `cargo test`
- `cargo fmt --check`
- `cargo build`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

Regression tests should verify:

- User messages render with a `❯ ` prompt marker.
- Multi-line user messages keep a continuation gutter.
- Assistant plain paragraphs render with a `• ` marker.
- Assistant headings, bullets, tables, quotes, and code blocks do not receive duplicate paragraph bullets.
- Existing Markdown-ish tests continue to pass.

## Risks

Prompt bars consume horizontal space and may wrap earlier in narrow terminals. This is acceptable because role separation is more important for transcript scanability. Keep the marker and gutter compact to minimize the impact.
