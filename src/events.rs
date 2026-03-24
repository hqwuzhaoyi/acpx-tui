use serde::Deserialize;
use std::io::BufRead;

#[derive(Debug, Clone)]
pub enum DisplayEvent {
    Message(String),
    ToolCall { title: String, kind: String },
    Thinking(String),
    Usage { used: u64, size: u64 },
}

impl std::fmt::Display for DisplayEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisplayEvent::Message(text) => write!(f, "💬 {}", truncate(text, 60)),
            DisplayEvent::ToolCall { title, kind } => {
                write!(f, "🔧 {}: {}", kind, truncate(title, 50))
            }
            DisplayEvent::Thinking(text) => write!(f, "💭 {}", truncate(text, 60)),
            DisplayEvent::Usage { used, size } => {
                let pct = if *size > 0 {
                    (*used as f64 / *size as f64 * 100.0) as u64
                } else {
                    0
                };
                write!(f, "📊 context: {}/{}  ({}%)", used, size, pct)
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > max {
        format!("{}...", s.chars().take(max).collect::<String>())
    } else {
        s
    }
}

/// Raw JSON-RPC message shape (only fields we care about)
#[derive(Deserialize)]
struct RpcMessage {
    method: Option<String>,
    params: Option<serde_json::Value>,
}

/// Parse a single NDJSON line into a DisplayEvent
pub fn parse_event(line: &str) -> Option<DisplayEvent> {
    let msg: RpcMessage = serde_json::from_str(line).ok()?;

    if msg.method.as_deref() != Some("session/update") {
        return None;
    }

    let params = msg.params?;
    let update = params.get("update")?;
    let session_update = update.get("sessionUpdate")?.as_str()?;

    match session_update {
        "agent_message_chunk" => {
            let text = update.get("content")?.get("text")?.as_str()?;
            if text.is_empty() {
                return None;
            }
            Some(DisplayEvent::Message(text.to_string()))
        }
        "tool_call" => {
            let title = update.get("title")?.as_str()?.to_string();
            let kind = update
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("tool")
                .to_string();
            Some(DisplayEvent::ToolCall { title, kind })
        }
        "agent_thought_chunk" => {
            let text = update.get("content")?.get("text")?.as_str()?;
            if text.len() < 10 {
                return None;
            }
            Some(DisplayEvent::Thinking(text.to_string()))
        }
        "usage_update" => {
            let used = update.get("used")?.as_u64()?;
            let size = update.get("size")?.as_u64()?;
            Some(DisplayEvent::Usage { used, size })
        }
        _ => None,
    }
}

/// Merge consecutive same-type chunks into single display events.
///
/// - Consecutive Message chunks → concatenated into one Message
/// - Consecutive Thinking chunks → concatenated into one Thinking
/// - Consecutive Usage with identical used/size → deduplicated
/// - ToolCall is never merged
pub fn merge_events(events: Vec<DisplayEvent>) -> Vec<DisplayEvent> {
    let mut merged: Vec<DisplayEvent> = Vec::new();

    for event in events {
        let should_merge = match (&event, merged.last()) {
            (DisplayEvent::Message(_), Some(DisplayEvent::Message(_))) => true,
            (DisplayEvent::Thinking(_), Some(DisplayEvent::Thinking(_))) => true,
            (
                DisplayEvent::Usage { used, size },
                Some(DisplayEvent::Usage {
                    used: prev_used,
                    size: prev_size,
                }),
            ) if used == prev_used && size == prev_size => true,
            _ => false,
        };

        if should_merge {
            match (event, merged.last_mut().unwrap()) {
                (DisplayEvent::Message(new_text), DisplayEvent::Message(ref mut acc)) => {
                    acc.push_str(&new_text);
                }
                (DisplayEvent::Thinking(new_text), DisplayEvent::Thinking(ref mut acc)) => {
                    acc.push_str(&new_text);
                }
                (DisplayEvent::Usage { .. }, _) => {
                    // Duplicate usage — skip
                }
                _ => unreachable!(),
            }
        } else {
            merged.push(event);
        }
    }

    merged
}

/// Load last N events from a .stream.ndjson file
pub fn load_recent_events(path: &str, max_events: usize) -> Vec<DisplayEvent> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };

    let reader = std::io::BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if let Some(event) = parse_event(&line) {
            events.push(event);
        }
    }

    // Merge consecutive same-type chunks before applying limit
    let mut events = merge_events(events);

    // Return last N
    if events.len() > max_events {
        events.split_off(events.len() - max_events)
    } else {
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_message_chunk() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello world"}}}}"#;
        let event = parse_event(line).unwrap();
        match event {
            DisplayEvent::Message(text) => assert_eq!(text, "Hello world"),
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_parse_agent_message_chunk_empty() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":""}}}}"#;
        assert!(parse_event(line).is_none());
    }

    #[test]
    fn test_parse_tool_call() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc","update":{"sessionUpdate":"tool_call","toolCallId":"call_123","title":"Read SKILL.md","kind":"read","status":"in_progress"}}}"#;
        let event = parse_event(line).unwrap();
        match event {
            DisplayEvent::ToolCall { title, kind } => {
                assert_eq!(title, "Read SKILL.md");
                assert_eq!(kind, "read");
            }
            _ => panic!("Expected ToolCall event"),
        }
    }

    #[test]
    fn test_parse_tool_call_no_kind() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc","update":{"sessionUpdate":"tool_call","toolCallId":"call_123","title":"Run test","status":"in_progress"}}}"#;
        let event = parse_event(line).unwrap();
        match event {
            DisplayEvent::ToolCall { title, kind } => {
                assert_eq!(title, "Run test");
                assert_eq!(kind, "tool");
            }
            _ => panic!("Expected ToolCall event"),
        }
    }

    #[test]
    fn test_parse_usage_update() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc","update":{"sessionUpdate":"usage_update","used":26169,"size":258400}}}"#;
        let event = parse_event(line).unwrap();
        match event {
            DisplayEvent::Usage { used, size } => {
                assert_eq!(used, 26169);
                assert_eq!(size, 258400);
            }
            _ => panic!("Expected Usage event"),
        }
    }

    #[test]
    fn test_parse_thought_chunk_short_skipped() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"hmm"}}}}"#;
        assert!(parse_event(line).is_none());
    }

    #[test]
    fn test_parse_thought_chunk_long_enough() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"Let me think about this problem carefully"}}}}"#;
        let event = parse_event(line).unwrap();
        match event {
            DisplayEvent::Thinking(text) => {
                assert_eq!(text, "Let me think about this problem carefully")
            }
            _ => panic!("Expected Thinking event"),
        }
    }

    #[test]
    fn test_parse_non_session_update_ignored() {
        let line = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":1}}"#;
        assert!(parse_event(line).is_none());
    }

    #[test]
    fn test_parse_unknown_session_update_ignored() {
        let line = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"abc","update":{"sessionUpdate":"available_commands_update","availableCommands":[]}}}"#;
        assert!(parse_event(line).is_none());
    }

    #[test]
    fn test_parse_invalid_json_ignored() {
        assert!(parse_event("not json").is_none());
        assert!(parse_event("").is_none());
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let long = "a".repeat(100);
        let result = truncate(&long, 10);
        assert_eq!(result.len(), 13); // 10 + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_cjk() {
        let cjk = "我会顺着现有的研究稿和相关实现痕迹往下梳理这个问题";
        let result = truncate(cjk, 10);
        assert_eq!(result.chars().count(), 13); // 10 chars + "..."
        assert!(result.ends_with("..."));
        assert!(result.starts_with("我会顺着现有的研究稿"));
    }

    #[test]
    fn test_truncate_newlines() {
        assert_eq!(truncate("hello\nworld", 20), "hello world");
    }

    #[test]
    fn test_display_event_message() {
        let e = DisplayEvent::Message("Hello".to_string());
        assert_eq!(format!("{}", e), "💬 Hello");
    }

    #[test]
    fn test_display_event_tool_call() {
        let e = DisplayEvent::ToolCall {
            title: "Read file.rs".to_string(),
            kind: "read".to_string(),
        };
        assert_eq!(format!("{}", e), "🔧 read: Read file.rs");
    }

    #[test]
    fn test_display_event_usage() {
        let e = DisplayEvent::Usage {
            used: 50000,
            size: 200000,
        };
        let s = format!("{}", e);
        assert!(s.contains("50000"));
        assert!(s.contains("200000"));
        assert!(s.contains("25%"));
    }

    #[test]
    fn test_load_recent_events_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stream.ndjson");
        let content = [
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}"#,
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello"}}}}"#,
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"c1","title":"Run cargo test","kind":"execute","status":"in_progress"}}}"#,
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"usage_update","used":1000,"size":10000}}}"#,
        ]
        .join("\n");
        std::fs::write(&path, content).unwrap();

        let events = load_recent_events(path.to_str().unwrap(), 10);
        assert_eq!(events.len(), 3); // message, tool_call, usage (initialize skipped)
    }

    #[test]
    fn test_load_recent_events_max_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.stream.ndjson");
        // Interleave messages with tool calls so they don't get merged
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!(
                r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s1","update":{{"sessionUpdate":"agent_message_chunk","content":{{"type":"text","text":"msg {i}"}}}}}}}}"#,
            ));
            lines.push(format!(
                r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s1","update":{{"sessionUpdate":"tool_call","toolCallId":"c{i}","title":"tool {i}","kind":"read","status":"in_progress"}}}}}}"#,
            ));
        }
        std::fs::write(&path, lines.join("\n")).unwrap();

        let events = load_recent_events(path.to_str().unwrap(), 3);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_load_recent_events_missing_file() {
        let events = load_recent_events("/nonexistent/file.ndjson", 10);
        assert!(events.is_empty());
    }

    #[test]
    fn test_merge_consecutive_messages() {
        let events = vec![
            DisplayEvent::Message("方".to_string()),
            DisplayEvent::Message("便".to_string()),
            DisplayEvent::Message("把".to_string()),
        ];
        let merged = merge_events(events);
        assert_eq!(merged.len(), 1);
        match &merged[0] {
            DisplayEvent::Message(text) => assert_eq!(text, "方便把"),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_merge_consecutive_thoughts() {
        let events = vec![
            DisplayEvent::Thinking("Let me ".to_string()),
            DisplayEvent::Thinking("think about ".to_string()),
            DisplayEvent::Thinking("this".to_string()),
        ];
        let merged = merge_events(events);
        assert_eq!(merged.len(), 1);
        match &merged[0] {
            DisplayEvent::Thinking(text) => assert_eq!(text, "Let me think about this"),
            _ => panic!("Expected Thinking"),
        }
    }

    #[test]
    fn test_merge_dedup_usage() {
        let events = vec![
            DisplayEvent::Usage {
                used: 52642,
                size: 258400,
            },
            DisplayEvent::Usage {
                used: 52642,
                size: 258400,
            },
        ];
        let merged = merge_events(events);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_merge_keeps_different_usage() {
        let events = vec![
            DisplayEvent::Usage {
                used: 52642,
                size: 258400,
            },
            DisplayEvent::Usage {
                used: 53074,
                size: 258400,
            },
        ];
        let merged = merge_events(events);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_tool_call_breaks_message_sequence() {
        let events = vec![
            DisplayEvent::Message("hello ".to_string()),
            DisplayEvent::Message("world".to_string()),
            DisplayEvent::ToolCall {
                title: "Read file".to_string(),
                kind: "read".to_string(),
            },
            DisplayEvent::Message("after ".to_string()),
            DisplayEvent::Message("tool".to_string()),
        ];
        let merged = merge_events(events);
        assert_eq!(merged.len(), 3);
        match &merged[0] {
            DisplayEvent::Message(text) => assert_eq!(text, "hello world"),
            _ => panic!("Expected Message"),
        }
        match &merged[2] {
            DisplayEvent::Message(text) => assert_eq!(text, "after tool"),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_merge_empty() {
        let merged = merge_events(vec![]);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_real_stream_files() {
        let home = std::env::var("HOME").unwrap_or_default();
        let sessions_dir = format!("{}/.acpx/sessions", home);
        let mut stream_files: Vec<String> = std::fs::read_dir(&sessions_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .to_str()
                    .unwrap_or("")
                    .ends_with(".stream.ndjson")
            })
            .map(|e| e.path().to_str().unwrap().to_string())
            .collect();
        stream_files.sort();
        stream_files.reverse(); // newest first by name

        for path in stream_files.iter().take(4) {
            let events = load_recent_events(path, 50);
            eprintln!(
                "\n=== {} === ({} events)",
                path.split('/').last().unwrap(),
                events.len()
            );
            for (i, e) in events.iter().enumerate() {
                let s = format!("{}", e);
                let display = if s.chars().count() > 80 {
                    format!("{}...", s.chars().take(80).collect::<String>())
                } else {
                    s
                };
                eprintln!("  [{:2}] {}", i, display);
            }
            // Should not panic — empty is OK if file has no session/update events
        }
    }

    #[test]
    fn test_merge_tool_calls_never_merged() {
        let events = vec![
            DisplayEvent::ToolCall {
                title: "Read a.rs".to_string(),
                kind: "read".to_string(),
            },
            DisplayEvent::ToolCall {
                title: "Read b.rs".to_string(),
                kind: "read".to_string(),
            },
        ];
        let merged = merge_events(events);
        assert_eq!(merged.len(), 2);
    }
}
