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
    if s.len() > max {
        format!("{}...", &s[..max])
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
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!(
                r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"s1","update":{{"sessionUpdate":"agent_message_chunk","content":{{"type":"text","text":"msg {i}"}}}}}}}}"#,
            ));
        }
        std::fs::write(&path, lines.join("\n")).unwrap();

        let events = load_recent_events(path.to_str().unwrap(), 3);
        assert_eq!(events.len(), 3);
        // Should be the last 3
        match &events[0] {
            DisplayEvent::Message(text) => assert_eq!(text, "msg 7"),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_load_recent_events_missing_file() {
        let events = load_recent_events("/nonexistent/file.ndjson", 10);
        assert!(events.is_empty());
    }
}
