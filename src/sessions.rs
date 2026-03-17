#![allow(dead_code)]

use crate::agents;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// ~/.acpx/sessions/index.json
#[derive(Debug, Deserialize)]
pub struct SessionIndex {
    pub entries: Vec<SessionIndexEntry>,
}

/// One entry in index.json (camelCase)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexEntry {
    pub file: String,
    pub acpx_record_id: String,
    pub acp_session_id: String,
    pub agent_command: String,
    pub cwd: String,
    pub closed: bool,
    pub last_used_at: String,
}

/// Full session detail from <id>.json (snake_case)
#[derive(Debug, Deserialize)]
pub struct SessionDetail {
    pub acpx_record_id: String,
    pub acp_session_id: String,
    pub agent_command: String,
    pub cwd: String,
    pub created_at: String,
    pub last_used_at: String,
    pub closed: bool,
    pub pid: Option<u32>,
    pub agent_started_at: Option<String>,
    pub last_agent_exit_at: Option<String>,
    pub last_agent_disconnect_reason: Option<String>,
    pub event_log: Option<EventLog>,
}

#[derive(Debug, Deserialize)]
pub struct EventLog {
    pub active_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Running,
    Exited,
    Closed,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Running => write!(f, "running"),
            SessionStatus::Exited => write!(f, "exited"),
            SessionStatus::Closed => write!(f, "closed"),
        }
    }
}

/// Resolved session info for display
#[derive(Debug, Clone)]
pub struct Session {
    pub acpx_record_id: String,
    pub acp_session_id: String,
    pub agent_type: String,
    pub cwd: String,
    pub status: SessionStatus,
    pub last_used_at: String,
    pub stream_path: Option<String>,
}

/// Parse agent type from agent_command string
/// "npx -y @zed-industries/claude-agent-acp@^0.21.0" -> "claude"
/// "npx @zed-industries/codex-acp@^0.9.5" -> "codex"
pub fn parse_agent_type(agent_command: &str) -> String {
    let cmd_lower = agent_command.to_lowercase();
    // Check all registered agents (ordering in AGENTS matters for substring safety)
    for agent in agents::AGENTS {
        if cmd_lower.contains(agent.name) {
            return agent.name.to_string();
        }
    }
    // Handle trae aliases (trae-cli, trae-agent) that contain "trae"
    if cmd_lower.contains("trae") {
        return "trae".to_string();
    }
    // Fallback: last token of the command
    agent_command
        .split_whitespace()
        .last()
        .unwrap_or("unknown")
        .to_string()
}

/// Check if pid is alive
fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Determine session status
pub fn resolve_status(detail: &SessionDetail) -> SessionStatus {
    if detail.closed {
        return SessionStatus::Closed;
    }
    if let Some(pid) = detail.pid {
        if is_pid_alive(pid) {
            return SessionStatus::Running;
        }
    }
    SessionStatus::Exited
}

fn default_sessions_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".acpx")
        .join("sessions")
}

/// Load all sessions from a directory
pub fn load_sessions_from(dir: &Path) -> Vec<Session> {
    let index_path = dir.join("index.json");

    let data = match std::fs::read_to_string(&index_path) {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    let index: SessionIndex = match serde_json::from_str(&data) {
        Ok(i) => i,
        Err(_) => return vec![],
    };

    index
        .entries
        .iter()
        .filter_map(|entry| {
            let detail_path = dir.join(&entry.file);
            let detail_data = std::fs::read_to_string(&detail_path).ok()?;
            let detail: SessionDetail = serde_json::from_str(&detail_data).ok()?;
            let status = resolve_status(&detail);
            let stream_path = detail.event_log.map(|e| e.active_path);

            Some(Session {
                acpx_record_id: entry.acpx_record_id.clone(),
                acp_session_id: entry.acp_session_id.clone(),
                agent_type: parse_agent_type(&entry.agent_command),
                cwd: entry.cwd.clone(),
                status,
                last_used_at: entry.last_used_at.clone(),
                stream_path,
            })
        })
        .collect()
}

/// Load all sessions from default ~/.acpx/sessions/ directory
pub fn load_sessions() -> Vec<Session> {
    load_sessions_from(&default_sessions_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_parse_index_json() {
        let json = r#"{
            "schema": "acpx.session-index.v1",
            "files": ["301a588a.json"],
            "entries": [{
                "file": "301a588a.json",
                "acpxRecordId": "301a588a-7ba2-4dc3-8abf-f78b02484fa7",
                "acpSessionId": "4ed50f0f-8a1d-41ec-a1ce-a59751baa957",
                "agentCommand": "npx -y @zed-industries/claude-agent-acp@^0.21.0",
                "cwd": "/Users/admin/workspace/project",
                "closed": false,
                "lastUsedAt": "2026-03-14T14:38:58.516Z"
            }]
        }"#;

        let index: SessionIndex = serde_json::from_str(json).unwrap();
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].acpx_record_id, "301a588a-7ba2-4dc3-8abf-f78b02484fa7");
        assert_eq!(index.entries[0].acp_session_id, "4ed50f0f-8a1d-41ec-a1ce-a59751baa957");
        assert!(!index.entries[0].closed);
    }

    #[test]
    fn test_parse_session_detail() {
        let json = r#"{
            "schema": "acpx.session.v1",
            "acpx_record_id": "019cec90-90d1-7511-b45a-a5748cd7437c",
            "acp_session_id": "019cec91-a677-7bc0-b98b-0ceb8f4e90ff",
            "agent_command": "npx @zed-industries/codex-acp@^0.9.5",
            "cwd": "/Users/admin/workspace/code-agent-monitor",
            "created_at": "2026-03-14T13:37:06.150Z",
            "last_used_at": "2026-03-14T13:39:29.045Z",
            "last_seq": 387,
            "closed": false,
            "pid": 73783,
            "agent_started_at": "2026-03-14T13:38:11.391Z",
            "last_agent_exit_at": "2026-03-14T13:44:29.147Z",
            "last_agent_disconnect_reason": "connection_close",
            "event_log": {
                "active_path": "/Users/admin/.acpx/sessions/019cec90.stream.ndjson",
                "segment_count": 5,
                "max_segment_bytes": 67108864
            }
        }"#;

        let detail: SessionDetail = serde_json::from_str(json).unwrap();
        assert_eq!(detail.acpx_record_id, "019cec90-90d1-7511-b45a-a5748cd7437c");
        assert_eq!(detail.pid, Some(73783));
        assert_eq!(
            detail.event_log.unwrap().active_path,
            "/Users/admin/.acpx/sessions/019cec90.stream.ndjson"
        );
    }

    #[test]
    fn test_parse_agent_type_claude() {
        assert_eq!(
            parse_agent_type("npx -y @zed-industries/claude-agent-acp@^0.21.0"),
            "claude"
        );
    }

    #[test]
    fn test_parse_agent_type_codex() {
        assert_eq!(
            parse_agent_type("npx @zed-industries/codex-acp@^0.9.5"),
            "codex"
        );
    }

    #[test]
    fn test_parse_agent_type_unknown() {
        assert_eq!(parse_agent_type("npx some-agent@1.0"), "some-agent@1.0");
    }

    #[test]
    fn test_resolve_status_closed() {
        let detail = SessionDetail {
            acpx_record_id: "id".into(),
            acp_session_id: "sid".into(),
            agent_command: "cmd".into(),
            cwd: "/tmp".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            last_used_at: "2026-01-01T00:00:00Z".into(),
            closed: true,
            pid: Some(99999),
            agent_started_at: None,
            last_agent_exit_at: None,
            last_agent_disconnect_reason: None,
            event_log: None,
        };
        assert_eq!(resolve_status(&detail), SessionStatus::Closed);
    }

    #[test]
    fn test_resolve_status_exited() {
        let detail = SessionDetail {
            acpx_record_id: "id".into(),
            acp_session_id: "sid".into(),
            agent_command: "cmd".into(),
            cwd: "/tmp".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            last_used_at: "2026-01-01T00:00:00Z".into(),
            closed: false,
            pid: Some(1), // PID 1 (launchd) is alive but we test with a dead PID
            agent_started_at: None,
            last_agent_exit_at: Some("2026-01-01T00:00:00Z".into()),
            last_agent_disconnect_reason: None,
            event_log: None,
        };
        // PID 1 is alive (launchd), so this will be Running on macOS
        // Use a definitely-dead PID instead
        let detail2 = SessionDetail {
            pid: Some(99999999),
            ..detail
        };
        assert_eq!(resolve_status(&detail2), SessionStatus::Exited);
    }

    #[test]
    fn test_load_sessions_from_dir() {
        let dir = create_test_dir();

        let index = r#"{
            "schema": "acpx.session-index.v1",
            "files": ["abc.json"],
            "entries": [{
                "file": "abc.json",
                "acpxRecordId": "abc",
                "acpSessionId": "sess-1",
                "agentCommand": "npx -y @zed-industries/claude-agent-acp@^0.21.0",
                "cwd": "/tmp/project",
                "closed": false,
                "lastUsedAt": "2026-03-14T14:00:00Z"
            }]
        }"#;
        fs::write(dir.path().join("index.json"), index).unwrap();

        let detail = r#"{
            "schema": "acpx.session.v1",
            "acpx_record_id": "abc",
            "acp_session_id": "sess-1",
            "agent_command": "npx -y @zed-industries/claude-agent-acp@^0.21.0",
            "cwd": "/tmp/project",
            "created_at": "2026-03-14T14:00:00Z",
            "last_used_at": "2026-03-14T14:00:00Z",
            "last_seq": 10,
            "closed": false,
            "pid": null,
            "agent_started_at": null,
            "last_agent_exit_at": "2026-03-14T14:05:00Z",
            "last_agent_disconnect_reason": null,
            "event_log": null
        }"#;
        fs::write(dir.path().join("abc.json"), detail).unwrap();

        let sessions = load_sessions_from(dir.path());
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].agent_type, "claude");
        assert_eq!(sessions[0].acp_session_id, "sess-1");
        assert_eq!(sessions[0].status, SessionStatus::Exited);
        assert!(sessions[0].stream_path.is_none());
    }

    #[test]
    fn test_load_sessions_missing_dir() {
        let sessions = load_sessions_from(Path::new("/nonexistent/path"));
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_session_status_display() {
        assert_eq!(SessionStatus::Running.to_string(), "running");
        assert_eq!(SessionStatus::Exited.to_string(), "exited");
        assert_eq!(SessionStatus::Closed.to_string(), "closed");
    }

    #[test]
    fn test_parse_agent_type_trae_cli() {
        assert_eq!(parse_agent_type("trae-cli acp serve"), "trae");
    }

    #[test]
    fn test_parse_agent_type_trae_agent_alias() {
        assert_eq!(parse_agent_type("trae-agent --resume abc"), "trae");
    }

    #[test]
    fn test_parse_agent_type_gemini() {
        assert_eq!(parse_agent_type("gemini --acp"), "gemini");
    }

    #[test]
    fn test_parse_agent_type_cursor() {
        assert_eq!(parse_agent_type("cursor-agent acp"), "cursor");
    }

    #[test]
    fn test_parse_agent_type_copilot() {
        assert_eq!(parse_agent_type("copilot --acp --stdio"), "copilot");
    }

    #[test]
    fn test_parse_agent_type_kimi() {
        assert_eq!(parse_agent_type("kimi acp"), "kimi");
    }

    #[test]
    fn test_parse_agent_type_kiro() {
        assert_eq!(parse_agent_type("kiro-cli acp"), "kiro");
    }

    #[test]
    fn test_parse_agent_type_qwen() {
        assert_eq!(parse_agent_type("qwen --acp"), "qwen");
    }

    #[test]
    fn test_parse_agent_type_droid() {
        assert_eq!(parse_agent_type("droid exec --output-format acp"), "droid");
    }

    #[test]
    fn test_parse_agent_type_iflow() {
        assert_eq!(parse_agent_type("iflow --experimental-acp"), "iflow");
    }

    #[test]
    fn test_parse_agent_type_kilocode() {
        assert_eq!(parse_agent_type("npx -y @kilocode/cli acp"), "kilocode");
    }

    #[test]
    fn test_parse_agent_type_opencode() {
        assert_eq!(parse_agent_type("npx -y opencode-ai acp"), "opencode");
    }

    #[test]
    fn test_parse_agent_type_pi() {
        assert_eq!(parse_agent_type("npx pi-acp"), "pi");
    }
}
