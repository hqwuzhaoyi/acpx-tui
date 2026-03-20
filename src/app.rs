use crate::events::{self, DisplayEvent};
use crate::sessions::{self, Session};

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub events: Vec<DisplayEvent>,
    pub should_quit: bool,
    pub show_details: bool,
    pub status_message: Option<String>,
    sessions_dir: Option<std::path::PathBuf>,
}

impl App {
    pub fn new() -> Self {
        let sessions = sessions::load_sessions();
        let events = sessions.first().map(load_events_for).unwrap_or_default();

        App {
            sessions,
            selected: 0,
            events,
            should_quit: false,
            show_details: false,
            status_message: None,
            sessions_dir: None,
        }
    }

    /// Create App with a custom sessions directory (for testing)
    #[cfg(test)]
    pub fn with_sessions_dir(dir: &std::path::Path) -> Self {
        let sessions = sessions::load_sessions_from(dir);
        let events = sessions.first().map(load_events_for).unwrap_or_default();

        App {
            sessions,
            selected: 0,
            events,
            should_quit: false,
            show_details: false,
            status_message: None,
            sessions_dir: Some(dir.to_path_buf()),
        }
    }

    pub fn refresh(&mut self) {
        self.sessions = if let Some(ref dir) = self.sessions_dir {
            sessions::load_sessions_from(dir)
        } else {
            sessions::load_sessions()
        };
        if self.selected >= self.sessions.len() && !self.sessions.is_empty() {
            self.selected = self.sessions.len() - 1;
        }
        self.reload_events();
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.sessions.get(self.selected)
    }

    pub fn select_next(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = (self.selected + 1).min(self.sessions.len() - 1);
            self.reload_events();
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.reload_events();
        }
    }

    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
    }

    pub fn set_status_message(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    fn reload_events(&mut self) {
        self.events = self
            .selected_session()
            .map(load_events_for)
            .unwrap_or_default();
    }
}

fn load_events_for(session: &Session) -> Vec<DisplayEvent> {
    session
        .stream_path
        .as_ref()
        .map(|p| events::load_recent_events(p, 50))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_sessions(count: usize) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();

        let mut entries = Vec::new();
        let mut files = Vec::new();

        for i in 0..count {
            let id = format!("session-{}", i);
            let filename = format!("{}.json", id);
            files.push(format!("\"{}\"", filename));

            entries.push(format!(
                r#"{{
                    "file": "{filename}",
                    "acpxRecordId": "{id}",
                    "acpSessionId": "acp-{id}",
                    "agentCommand": "npx -y @zed-industries/claude-agent-acp@^0.21.0",
                    "cwd": "/tmp/project-{i}",
                    "closed": false,
                    "lastUsedAt": "2026-03-14T14:00:0{i}Z"
                }}"#
            ));

            let detail = format!(
                r#"{{
                    "schema": "acpx.session.v1",
                    "acpx_record_id": "{id}",
                    "acp_session_id": "acp-{id}",
                    "agent_command": "npx -y @zed-industries/claude-agent-acp@^0.21.0",
                    "cwd": "/tmp/project-{i}",
                    "created_at": "2026-03-14T14:00:0{i}Z",
                    "last_used_at": "2026-03-14T14:00:0{i}Z",
                    "last_seq": 10,
                    "closed": false,
                    "pid": null,
                    "agent_started_at": null,
                    "last_agent_exit_at": "2026-03-14T14:05:0{i}Z",
                    "last_agent_disconnect_reason": null,
                    "event_log": null
                }}"#
            );
            fs::write(dir.path().join(&filename), detail).unwrap();
        }

        let index = format!(
            r#"{{"schema": "acpx.session-index.v1", "files": [{}], "entries": [{}]}}"#,
            files.join(","),
            entries.join(",")
        );
        fs::write(dir.path().join("index.json"), index).unwrap();

        dir
    }

    #[test]
    fn test_app_new_empty() {
        let dir = tempfile::tempdir().unwrap();
        // No index.json
        let app = App::with_sessions_dir(dir.path());
        assert!(app.sessions.is_empty());
        assert_eq!(app.selected, 0);
        assert!(app.events.is_empty());
        assert!(!app.should_quit);
        assert!(!app.show_details);
    }

    #[test]
    fn test_app_with_sessions() {
        let dir = setup_test_sessions(3);
        let app = App::with_sessions_dir(dir.path());
        assert_eq!(app.sessions.len(), 3);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_next() {
        let dir = setup_test_sessions(3);
        let mut app = App::with_sessions_dir(dir.path());

        app.select_next();
        assert_eq!(app.selected, 1);

        app.select_next();
        assert_eq!(app.selected, 2);

        // Should not go beyond last
        app.select_next();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn test_select_prev() {
        let dir = setup_test_sessions(3);
        let mut app = App::with_sessions_dir(dir.path());

        app.selected = 2;
        app.select_prev();
        assert_eq!(app.selected, 1);

        app.select_prev();
        assert_eq!(app.selected, 0);

        // Should not go below 0
        app.select_prev();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_next_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_sessions_dir(dir.path());
        app.select_next(); // Should not panic
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_toggle_details() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_sessions_dir(dir.path());

        assert!(!app.show_details);
        app.toggle_details();
        assert!(app.show_details);
        app.toggle_details();
        assert!(!app.show_details);
    }

    #[test]
    fn test_selected_session() {
        let dir = setup_test_sessions(2);
        let mut app = App::with_sessions_dir(dir.path());

        assert_eq!(
            app.selected_session().unwrap().acp_session_id,
            "acp-session-0"
        );
        app.select_next();
        assert_eq!(
            app.selected_session().unwrap().acp_session_id,
            "acp-session-1"
        );
    }

    #[test]
    fn test_selected_session_empty() {
        let dir = tempfile::tempdir().unwrap();
        let app = App::with_sessions_dir(dir.path());
        assert!(app.selected_session().is_none());
    }

    #[test]
    fn test_refresh_clamps_selected() {
        let dir = setup_test_sessions(3);
        let mut app = App::with_sessions_dir(dir.path());

        app.selected = 2;

        // Rewrite index with only 1 session
        let index = r#"{"schema":"v1","files":["session-0.json"],"entries":[{
            "file":"session-0.json","acpxRecordId":"session-0","acpSessionId":"acp-session-0",
            "agentCommand":"npx -y @zed-industries/claude-agent-acp@^0.21.0",
            "cwd":"/tmp/project-0","closed":false,"lastUsedAt":"2026-03-14T14:00:00Z"
        }]}"#;
        fs::write(dir.path().join("index.json"), index).unwrap();

        app.refresh();
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.selected, 0); // Clamped from 2 to 0
    }

    #[test]
    fn test_status_message() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_sessions_dir(dir.path());

        assert!(app.status_message.is_none());
        app.set_status_message("test message".to_string());
        assert_eq!(app.status_message.as_deref(), Some("test message"));
        app.clear_status_message();
        assert!(app.status_message.is_none());
    }
}
