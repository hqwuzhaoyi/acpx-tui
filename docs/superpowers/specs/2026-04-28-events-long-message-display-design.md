# Events Long Message Display Design

## Problem

The Events panel truncates message text before rendering it. Long assistant or user messages, including API usage descriptions such as `/yzg-saas-trans-app/yzgApp/supplement/create`, lose their later content permanently instead of becoming available through wrapping or scrolling.

## Goal

Make long conversational event content readable in the Events panel while preserving a compact display for operational events.

## Scope

In scope:

- Assistant messages, user messages, and thinking messages should render their full text.
- Tool call titles should remain abbreviated.
- Usage events should remain unchanged.
- Existing Events panel wrapping and scrolling should continue to handle vertical overflow.
- Add regression coverage for long text, including CJK text.

Out of scope:

- A per-event expand/collapse mode.
- New keybindings.
- Horizontal scrolling.
- Changes to event parsing or merge behavior.

## Design

Use a mixed display strategy:

- `DisplayEvent::Message`, `DisplayEvent::UserMessage`, and `DisplayEvent::Thinking` render complete text after normalizing embedded newlines to spaces.
- `DisplayEvent::ToolCall` continues to truncate the title to keep tool-heavy sessions scan-friendly.
- `DisplayEvent::Usage` keeps the current context summary format.

The UI already renders Events with `Paragraph::wrap` and applies `app.event_scroll`, so no layout rewrite is needed. Removing message truncation lets wrapped lines occupy more vertical space, and the existing `j`/`k` keys plus mouse wheel expose the rest of the content.

## Components

- `src/events.rs`
  - Add or reuse a helper that normalizes message text onto one logical line without truncating it.
  - Keep `truncate` for compact event fields, especially tool call titles.
  - Update display tests so long messages are expected to remain intact.

- `src/ui.rs`
  - No planned change. Existing wrapping and scrolling remain the rendering mechanism.

## Data Flow

1. Event stream lines are parsed into `DisplayEvent`.
2. Consecutive chunks are merged as they are today.
3. `DisplayEvent` formats itself for display.
4. Events panel wraps the resulting text to the available width and scrolls vertically through overflow.

## Error Handling

No new error cases are introduced. Invalid stream lines and missing files keep the existing behavior.

## Testing

Run `cargo test`.

Regression tests should verify:

- Long assistant message text is not truncated.
- Long CJK text is not truncated.
- Tool call titles are still truncated.
- Newlines in message text are normalized to spaces.

## Risks

Long merged messages can consume more vertical space in the Events panel. This is accepted because the user need is to inspect full content, and the panel already supports scrolling.
