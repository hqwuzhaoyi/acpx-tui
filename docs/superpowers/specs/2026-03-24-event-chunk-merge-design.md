# Event Chunk Merge Design

## Problem

ACP streams `agent_message_chunk` and `agent_thought_chunk` as individual tokens (often single characters for CJK text). The TUI displays each token as a separate line, making the events panel unreadable. Additionally, `usage_update` events often arrive in consecutive duplicates.

## Solution

Post-process merge in `events.rs`: after parsing all raw events, run a `merge_events()` pass that combines consecutive same-type chunks into single display events.

## Merge Rules

1. Consecutive `Message` + `Message` → concatenate text
2. Consecutive `Thinking` + `Thinking` → concatenate text
3. Consecutive `Usage` with identical `used`/`size` → keep only one
4. `ToolCall` — never merged, always standalone
5. Any type change → start a new event

## Changes

**File:** `src/events.rs`

- Add `pub fn merge_events(events: Vec<DisplayEvent>) -> Vec<DisplayEvent>`
- Update `load_recent_events` to call `merge_events` before applying `max_events` limit
- Add unit tests for merge behavior

## Behavior

```
Before:                          After:
💬 方                            💬 方便把"可复用能力"写细。
💬 便                            🔧 search: Search *.vue...
💬 把                            📊 context: 52642/258400 (20%)
...12 lines...                   🔧 read: Read index.vue...
🔧 search: Search *.vue...
📊 context: 52642/258400 (20%)
📊 context: 52642/258400 (20%)
🔧 read: Read index.vue...
```

The `max_events` limit applies after merging, so 50 events = 50 logical events, not 50 tokens.
