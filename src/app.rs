use crate::acpx_control;
use crate::events::{self, DisplayEvent};
use crate::launcher::{LaunchRequest, LauncherState};
use crate::prompt_editor::PromptEditor;
use crate::prompt_history::{HistoryNavigation, PromptHistory};
use crate::sessions::{self, Session};
use ratatui::widgets::ListState;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Sessions,
    Events,
    Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Prompt,
}

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub list_state: ListState,
    pub events: Vec<DisplayEvent>,
    pub should_quit: bool,
    pub show_details: bool,
    pub status_message: Option<String>,
    pub event_scroll: u16,
    pub focused_panel: Panel,
    pub confirm_delete: bool,
    pub input_mode: InputMode,
    pub prompt_editor: PromptEditor,
    pub prompt_send_in_flight: bool,
    prompt_history: PromptHistory,
    prompt_history_nav: HistoryNavigation,
    pub launcher: Option<LauncherState>,
    pub launcher_launch_in_flight: bool,
    sessions_dir: Option<std::path::PathBuf>,
}

impl App {
    pub fn new() -> Self {
        let sessions = sessions::load_sessions();
        let events = sessions.first().map(load_events_for).unwrap_or_default();
        let mut list_state = ListState::default();
        if !sessions.is_empty() {
            list_state.select(Some(0));
        }
        let (prompt_history, history_warning) = PromptHistory::load_default();

        let mut app = App {
            sessions,
            selected: 0,
            list_state,
            events,
            should_quit: false,
            show_details: false,
            status_message: None,
            event_scroll: 0,
            focused_panel: Panel::Sessions,
            confirm_delete: false,
            input_mode: InputMode::Normal,
            prompt_editor: PromptEditor::new(),
            prompt_send_in_flight: false,
            prompt_history,
            prompt_history_nav: HistoryNavigation::default(),
            launcher: None,
            launcher_launch_in_flight: false,
            sessions_dir: None,
        };
        if let Some(warning) = history_warning {
            app.set_status_message(warning);
        }
        app
    }

    /// Create App with a custom sessions directory (for testing)
    #[cfg(test)]
    pub fn with_sessions_dir(dir: &std::path::Path) -> Self {
        let sessions = sessions::load_sessions_from(dir);
        let events = sessions.first().map(load_events_for).unwrap_or_default();
        let mut list_state = ListState::default();
        if !sessions.is_empty() {
            list_state.select(Some(0));
        }
        let (prompt_history, history_warning) =
            PromptHistory::load_from(dir.join("prompt-history.json"));

        let mut app = App {
            sessions,
            selected: 0,
            list_state,
            events,
            should_quit: false,
            show_details: false,
            status_message: None,
            event_scroll: 0,
            focused_panel: Panel::Sessions,
            confirm_delete: false,
            input_mode: InputMode::Normal,
            prompt_editor: PromptEditor::new(),
            prompt_send_in_flight: false,
            prompt_history,
            prompt_history_nav: HistoryNavigation::default(),
            launcher: None,
            launcher_launch_in_flight: false,
            sessions_dir: Some(dir.to_path_buf()),
        };
        if let Some(warning) = history_warning {
            app.set_status_message(warning);
        }
        app
    }

    pub fn refresh(&mut self) {
        let previous_selected_id = self
            .selected_session()
            .map(|session| session.acpx_record_id.clone());
        let previous_event_scroll = self.event_scroll;

        self.sessions = if let Some(ref dir) = self.sessions_dir {
            sessions::load_sessions_from(dir)
        } else {
            sessions::load_sessions()
        };
        if self.selected >= self.sessions.len() && !self.sessions.is_empty() {
            self.selected = self.sessions.len() - 1;
        }
        self.list_state.select(if self.sessions.is_empty() {
            None
        } else {
            Some(self.selected)
        });
        let selected_session_unchanged = previous_selected_id.as_deref().is_some_and(|id| {
            self.selected_session()
                .is_some_and(|session| session.acpx_record_id == id)
        });
        self.reload_events();
        if selected_session_unchanged {
            self.event_scroll = previous_event_scroll;
        }
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.sessions.get(self.selected)
    }

    pub fn select_next(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = (self.selected + 1) % self.sessions.len();
            self.list_state.select(Some(self.selected));
            self.prompt_history_nav.reset();
            self.reload_events();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = if self.selected == 0 {
                self.sessions.len() - 1
            } else {
                self.selected - 1
            };
            self.list_state.select(Some(self.selected));
            self.prompt_history_nav.reset();
            self.reload_events();
        }
    }

    pub fn toggle_details(&mut self) {
        self.show_details = !self.show_details;
        self.event_scroll = 0;
    }

    pub fn scroll_events_up(&mut self) {
        self.event_scroll = self.event_scroll.saturating_sub(3);
    }

    pub fn scroll_events_down(&mut self) {
        self.event_scroll = self.event_scroll.saturating_add(3);
    }

    pub fn toggle_panel(&mut self) {
        self.focused_panel = match self.focused_panel {
            Panel::Sessions => Panel::Events,
            Panel::Events => Panel::Prompt,
            Panel::Prompt => Panel::Sessions,
        };
        self.input_mode = if self.focused_panel == Panel::Prompt {
            InputMode::Prompt
        } else {
            InputMode::Normal
        };
    }

    pub fn set_status_message(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    pub fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    pub fn request_delete(&mut self) {
        if self.selected_session().is_some() {
            self.confirm_delete = true;
        }
    }

    pub fn confirm_delete_yes(&mut self) {
        self.confirm_delete = false;
        if let Some(session) = self.selected_session().cloned() {
            let dir = self.sessions_dir.clone().unwrap_or_else(|| {
                dirs::home_dir()
                    .expect("no home dir")
                    .join(".acpx")
                    .join("sessions")
            });
            match sessions::delete_session(&dir, &session.acpx_record_id) {
                Ok(()) => {
                    self.set_status_message("Session deleted".to_string());
                    self.refresh();
                }
                Err(e) => {
                    self.set_status_message(format!("Delete failed: {}", e));
                }
            }
        }
    }

    pub fn cancel_delete(&mut self) {
        self.confirm_delete = false;
    }

    pub fn start_prompt_input(&mut self) {
        if self.prompt_send_in_flight {
            self.set_status_message("Prompt send already in progress".to_string());
            return;
        }
        if self.selected_session().is_none() {
            self.set_status_message("No session selected".to_string());
            return;
        }

        self.confirm_delete = false;
        self.input_mode = InputMode::Prompt;
        self.focused_panel = Panel::Prompt;
    }

    pub fn push_prompt_char(&mut self, c: char) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.insert_char(c);
            self.prompt_history_nav.reset();
        }
    }

    pub fn insert_prompt_newline(&mut self) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.insert_newline();
            self.prompt_history_nav.reset();
        }
    }

    pub fn backspace_prompt(&mut self) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.delete_before_cursor();
            self.prompt_history_nav.reset();
        }
    }

    pub fn delete_prompt_after_cursor(&mut self) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.delete_after_cursor();
            self.prompt_history_nav.reset();
        }
    }

    pub fn delete_prompt_to_line_start(&mut self) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.delete_to_line_start();
            self.prompt_history_nav.reset();
        }
    }

    pub fn delete_prompt_to_buffer_start(&mut self) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.delete_to_buffer_start();
            self.prompt_history_nav.reset();
        }
    }

    pub fn delete_prompt_to_line_end(&mut self) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.delete_to_line_end();
            self.prompt_history_nav.reset();
        }
    }

    pub fn delete_prompt_word_before(&mut self) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.delete_word_before();
            self.prompt_history_nav.reset();
        }
    }

    pub fn move_prompt_left(&mut self) {
        self.prompt_editor.move_left();
        self.prompt_history_nav.reset();
    }

    pub fn move_prompt_right(&mut self) {
        self.prompt_editor.move_right();
        self.prompt_history_nav.reset();
    }

    pub fn move_prompt_to_line_start(&mut self) {
        self.prompt_editor.move_to_line_start();
        self.prompt_history_nav.reset();
    }

    pub fn move_prompt_to_line_end(&mut self) {
        self.prompt_editor.move_to_line_end();
        self.prompt_history_nav.reset();
    }

    pub fn paste_prompt(&mut self, text: &str) {
        if self.input_mode == InputMode::Prompt {
            self.prompt_editor.insert_str(text);
            self.prompt_history_nav.reset();
        }
    }

    pub fn clear_prompt_buffer(&mut self) {
        self.prompt_editor.clear();
        self.prompt_history_nav.reset();
        self.set_status_message("Prompt cleared".to_string());
    }

    pub fn handle_prompt_interrupt(&mut self) {
        if self.prompt_editor.is_empty() {
            self.should_quit = true;
        } else {
            self.clear_prompt_buffer();
        }
    }

    pub fn prompt_up(&mut self) {
        if self.prompt_editor.is_at_start() {
            self.recall_previous_prompt();
        } else {
            self.prompt_editor.move_up();
            self.prompt_history_nav.reset();
        }
    }

    pub fn prompt_down(&mut self) {
        if self.prompt_editor.is_at_end() {
            self.recall_next_prompt();
        } else {
            self.prompt_editor.move_down();
            self.prompt_history_nav.reset();
        }
    }

    pub fn cancel_prompt_input(&mut self) {
        self.prompt_editor.clear();
        self.prompt_history_nav.reset();
        self.input_mode = InputMode::Normal;
        self.focused_panel = Panel::Events;
        self.set_status_message("Prompt cancelled".to_string());
    }

    pub fn submit_prompt_input(&mut self) -> Option<(Session, String)> {
        if self.prompt_send_in_flight {
            self.set_status_message("Prompt send already in progress".to_string());
            return None;
        }

        let prompt = self.prompt_editor.text();
        if prompt.trim().is_empty() {
            self.set_status_message("Prompt is empty".to_string());
            return None;
        }

        let session = match self.selected_session().cloned() {
            Some(session) => session,
            None => {
                self.set_status_message("No session selected".to_string());
                return None;
            }
        };

        let history_result = self
            .prompt_history
            .record(&acpx_control::prompt_session_selector(&session), &prompt);
        self.prompt_editor.clear();
        self.prompt_history_nav.reset();
        self.input_mode = InputMode::Prompt;
        self.focused_panel = Panel::Prompt;
        self.prompt_send_in_flight = true;
        let target = session.name.as_deref().unwrap_or(&session.acp_session_id);
        match history_result {
            Ok(()) => self.set_status_message(format!("Sending prompt to {}...", target)),
            Err(error) => self.set_status_message(format!(
                "Sending prompt to {}... (history not saved: {})",
                target, error
            )),
        }
        Some((session, prompt))
    }

    pub fn complete_prompt_send(&mut self, result: Result<String, String>) {
        self.prompt_send_in_flight = false;
        match result {
            Ok(message) => {
                self.set_status_message(message);
                self.refresh();
            }
            Err(error) => {
                self.set_status_message(format!("Send failed: {}", error));
            }
        }
    }

    pub fn open_launcher(&mut self, agents: Vec<String>, roots: Vec<PathBuf>) {
        if self.launcher_launch_in_flight {
            self.set_status_message("Session creation already in progress".to_string());
            return;
        }
        if agents.is_empty() {
            self.set_status_message("No acpx agents found".to_string());
            return;
        }

        self.confirm_delete = false;
        self.input_mode = InputMode::Normal;
        self.launcher = Some(LauncherState::new(agents, roots));
        self.set_status_message("Choose a directory for the new session".to_string());
    }

    pub fn launcher_is_active(&self) -> bool {
        self.launcher.is_some()
    }

    pub fn cancel_launcher(&mut self) {
        self.launcher = None;
        self.set_status_message("New session cancelled".to_string());
    }

    pub fn push_launcher_char(&mut self, c: char) {
        if let Some(launcher) = &mut self.launcher {
            launcher.push_char(c);
        }
    }

    pub fn paste_launcher(&mut self, text: &str) {
        if let Some(launcher) = &mut self.launcher {
            launcher.paste(text);
        }
    }

    pub fn backspace_launcher(&mut self) {
        if let Some(launcher) = &mut self.launcher {
            launcher.backspace();
        }
    }

    pub fn select_next_launcher_item(&mut self) {
        if let Some(launcher) = &mut self.launcher {
            launcher.select_next();
        }
    }

    pub fn select_prev_launcher_item(&mut self) {
        if let Some(launcher) = &mut self.launcher {
            launcher.select_prev();
        }
    }

    pub fn confirm_launcher(&mut self) -> Option<LaunchRequest> {
        if self.launcher_launch_in_flight {
            self.set_status_message("Session creation already in progress".to_string());
            return None;
        }

        let Some(launcher) = &mut self.launcher else {
            return None;
        };

        match launcher.confirm() {
            Ok(Some(request)) => {
                self.launcher = None;
                self.launcher_launch_in_flight = true;
                self.set_status_message(format!(
                    "Creating {} session in {}...",
                    request.agent,
                    request.cwd.display()
                ));
                Some(request)
            }
            Ok(None) => {
                self.set_status_message("Choose an agent for the new session".to_string());
                None
            }
            Err(message) => {
                self.set_status_message(message);
                None
            }
        }
    }

    pub fn complete_session_create(&mut self, result: Result<String, String>) {
        self.launcher_launch_in_flight = false;
        match result {
            Ok(message) => {
                let previous_ids: HashSet<String> = self
                    .sessions
                    .iter()
                    .map(|session| session.acpx_record_id.clone())
                    .collect();
                self.refresh();
                if let Some(index) = self
                    .sessions
                    .iter()
                    .position(|session| !previous_ids.contains(&session.acpx_record_id))
                {
                    self.selected = index;
                    self.list_state.select(Some(index));
                    self.reload_events();
                }
                self.set_status_message(message);
            }
            Err(error) => {
                self.set_status_message(format!("Create failed: {}", error));
            }
        }
    }

    fn reload_events(&mut self) {
        self.events = self
            .selected_session()
            .map(load_events_for)
            .unwrap_or_default();
        self.event_scroll = 0;
    }

    fn selected_prompt_key(&self) -> Option<String> {
        self.selected_session()
            .map(acpx_control::prompt_session_selector)
    }

    fn recall_previous_prompt(&mut self) {
        if let Some(key) = self.selected_prompt_key() {
            let current = self.prompt_editor.text();
            if let Some(prompt) =
                self.prompt_history_nav
                    .previous(&key, &current, self.prompt_history.entries(&key))
            {
                self.prompt_editor.set_text(&prompt);
            }
        }
    }

    fn recall_next_prompt(&mut self) {
        if let Some(key) = self.selected_prompt_key() {
            let current = self.prompt_editor.text();
            if let Some(prompt) =
                self.prompt_history_nav
                    .next(&key, &current, self.prompt_history.entries(&key))
            {
                self.prompt_editor.set_text(&prompt);
            }
        }
    }
}

fn load_events_for(session: &Session) -> Vec<DisplayEvent> {
    // Try acpx stream first
    if let Some(ref path) = session.stream_path {
        let events = events::load_recent_events(path, 50);
        if !events.is_empty() {
            return events;
        }
    }

    // Fallback: try openclaw stream
    if let Some(oc_path) = sessions::resolve_openclaw_stream(session) {
        if let Some(path_str) = oc_path.to_str() {
            let events = events::load_openclaw_events(path_str, 50);
            if !events.is_empty() {
                return events;
            }
        }
    }

    vec![]
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
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(!app.prompt_send_in_flight);
        assert!(app.prompt_editor.is_empty());
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

        // Wraps to beginning
        app.select_next();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_prev() {
        let dir = setup_test_sessions(3);
        let mut app = App::with_sessions_dir(dir.path());

        // Wraps to end
        app.select_prev();
        assert_eq!(app.selected, 2);

        app.select_prev();
        assert_eq!(app.selected, 1);

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
        app.scroll_events_down();
        assert_eq!(app.event_scroll, 3);

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
        assert_eq!(app.event_scroll, 0);
    }

    #[test]
    fn test_refresh_preserves_event_scroll_for_same_session() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        app.scroll_events_down();
        app.scroll_events_down();
        assert_eq!(app.event_scroll, 6);

        app.refresh();

        assert_eq!(app.selected_session().unwrap().acpx_record_id, "session-0");
        assert_eq!(app.event_scroll, 6);
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

    #[test]
    fn test_launcher_confirm_builds_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_sessions_dir(dir.path());
        app.open_launcher(vec!["codex".to_string()], vec![dir.path().to_path_buf()]);

        assert!(app.launcher_is_active());
        assert!(app.confirm_launcher().is_none());
        let request = app.confirm_launcher().unwrap();
        assert_eq!(request.agent, "codex");
        assert_eq!(request.cwd, dir.path().canonicalize().unwrap());
        assert!(app.launcher_launch_in_flight);
    }

    #[test]
    fn test_complete_session_create_clears_in_flight() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_sessions_dir(dir.path());
        app.launcher_launch_in_flight = true;

        app.complete_session_create(Ok("Created codex session".to_string()));

        assert!(!app.launcher_launch_in_flight);
        assert_eq!(app.status_message.as_deref(), Some("Created codex session"));
    }

    #[test]
    fn test_request_delete_empty() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::with_sessions_dir(dir.path());
        app.request_delete();
        assert!(!app.confirm_delete); // No sessions, should not enter confirm mode
    }

    #[test]
    fn test_request_delete_with_sessions() {
        let dir = setup_test_sessions(2);
        let mut app = App::with_sessions_dir(dir.path());
        app.request_delete();
        assert!(app.confirm_delete);
    }

    #[test]
    fn test_cancel_delete() {
        let dir = setup_test_sessions(2);
        let mut app = App::with_sessions_dir(dir.path());
        app.request_delete();
        assert!(app.confirm_delete);
        app.cancel_delete();
        assert!(!app.confirm_delete);
    }

    #[test]
    fn test_toggle_panel_cycles_through_prompt_composer() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        assert_eq!(app.focused_panel, Panel::Sessions);
        assert_eq!(app.input_mode, InputMode::Normal);

        app.toggle_panel();
        assert_eq!(app.focused_panel, Panel::Events);
        assert_eq!(app.input_mode, InputMode::Normal);

        app.toggle_panel();
        assert_eq!(app.focused_panel, Panel::Prompt);
        assert_eq!(app.input_mode, InputMode::Prompt);

        app.toggle_panel();
        assert_eq!(app.focused_panel, Panel::Sessions);
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_prompt_input_lifecycle() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        app.start_prompt_input();
        assert_eq!(app.input_mode, InputMode::Prompt);
        assert_eq!(app.focused_panel, Panel::Prompt);

        app.push_prompt_char('h');
        app.push_prompt_char('i');
        app.paste_prompt(" there");
        app.backspace_prompt();
        app.push_prompt_char(' ');

        let (session, prompt) = app.submit_prompt_input().unwrap();
        assert_eq!(session.acp_session_id, "acp-session-0");
        assert_eq!(prompt, "hi ther ");
        assert_eq!(app.input_mode, InputMode::Prompt);
        assert_eq!(app.focused_panel, Panel::Prompt);
        assert!(app.prompt_send_in_flight);

        app.complete_prompt_send(Ok("Prompt sent".to_string()));
        assert!(!app.prompt_send_in_flight);
        assert_eq!(app.status_message.as_deref(), Some("Prompt sent"));
    }

    #[test]
    fn test_prompt_paste_preserves_multiline_text_until_submit() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        app.start_prompt_input();
        app.paste_prompt(
            "/yzg-saas-trans-app/yzgApp/supplement/loadMybOrder\n\
             /yzg-saas-trans-app/yzgApp/supplement/create",
        );

        assert_eq!(
            app.prompt_editor.text(),
            "/yzg-saas-trans-app/yzgApp/supplement/loadMybOrder\n/yzg-saas-trans-app/yzgApp/supplement/create"
        );
        assert!(!app.prompt_send_in_flight);

        let (_session, prompt) = app.submit_prompt_input().unwrap();
        assert_eq!(
            prompt,
            "/yzg-saas-trans-app/yzgApp/supplement/loadMybOrder\n/yzg-saas-trans-app/yzgApp/supplement/create"
        );
    }

    #[test]
    fn test_prompt_submit_continues_when_history_persistence_fails() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());
        app.prompt_history =
            PromptHistory::load_from(dir.path().join("missing").join("history.json")).0;
        fs::write(dir.path().join("missing"), "not a directory").unwrap();

        app.start_prompt_input();
        app.paste_prompt("still send");
        let (_session, prompt) = app.submit_prompt_input().unwrap();

        assert_eq!(prompt, "still send");
        assert!(app.prompt_send_in_flight);
        assert!(app
            .status_message
            .as_deref()
            .unwrap()
            .contains("history not saved"));
    }

    #[test]
    fn test_prompt_input_rejects_empty_prompt() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        app.start_prompt_input();
        assert!(app.submit_prompt_input().is_none());
        assert_eq!(app.input_mode, InputMode::Prompt);
        assert_eq!(app.status_message.as_deref(), Some("Prompt is empty"));
    }

    #[test]
    fn test_prompt_input_rejects_second_send_while_in_flight() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        app.start_prompt_input();
        app.paste_prompt("first");
        assert!(app.submit_prompt_input().is_some());

        app.paste_prompt("second");
        assert!(app.submit_prompt_input().is_none());
        assert_eq!(
            app.status_message.as_deref(),
            Some("Prompt send already in progress")
        );
    }

    #[test]
    fn test_prompt_history_navigation_and_ctrl_c_clear_state() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        app.start_prompt_input();
        app.paste_prompt("first");
        assert!(app.submit_prompt_input().is_some());
        app.complete_prompt_send(Ok("Prompt sent".to_string()));

        app.paste_prompt("draft");
        app.prompt_editor.move_to_line_start();
        app.prompt_up();
        assert_eq!(app.prompt_editor.text(), "first");

        app.prompt_editor.move_to_line_end();
        app.prompt_down();
        assert_eq!(app.prompt_editor.text(), "draft");

        app.clear_prompt_buffer();
        assert!(app.prompt_editor.is_empty());
        assert_eq!(app.status_message.as_deref(), Some("Prompt cleared"));
    }

    #[test]
    fn test_prompt_interrupt_clears_non_empty_buffer_then_exits_when_empty() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        app.start_prompt_input();
        app.paste_prompt("draft");
        app.handle_prompt_interrupt();

        assert!(app.prompt_editor.is_empty());
        assert!(!app.should_quit);
        assert_eq!(app.status_message.as_deref(), Some("Prompt cleared"));

        app.handle_prompt_interrupt();
        assert!(app.should_quit);
    }

    #[test]
    fn test_prompt_accepts_newline_for_shift_enter() {
        let dir = setup_test_sessions(1);
        let mut app = App::with_sessions_dir(dir.path());

        app.start_prompt_input();
        app.push_prompt_char('a');
        app.push_prompt_char('\n');
        app.push_prompt_char('b');

        assert_eq!(app.prompt_editor.text(), "a\nb");
    }

    #[test]
    fn test_prompt_history_is_session_scoped() {
        let dir = setup_test_sessions(2);
        let mut app = App::with_sessions_dir(dir.path());

        app.start_prompt_input();
        app.paste_prompt("session one");
        assert!(app.submit_prompt_input().is_some());
        app.complete_prompt_send(Ok("Prompt sent".to_string()));

        app.select_next();
        app.start_prompt_input();
        app.prompt_up();
        assert!(app.prompt_editor.is_empty());

        app.paste_prompt("session two");
        assert!(app.submit_prompt_input().is_some());
        app.complete_prompt_send(Ok("Prompt sent".to_string()));

        app.prompt_up();
        assert_eq!(app.prompt_editor.text(), "session two");
    }

    #[test]
    fn test_confirm_delete_yes() {
        let dir = setup_test_sessions(3);
        let mut app = App::with_sessions_dir(dir.path());
        assert_eq!(app.sessions.len(), 3);

        app.request_delete();
        app.confirm_delete_yes();

        assert!(!app.confirm_delete);
        assert_eq!(app.sessions.len(), 2);
        assert_eq!(app.status_message.as_deref(), Some("Session deleted"));
    }

    #[test]
    fn test_load_events_for_acpx_stream() {
        let dir = tempfile::tempdir().unwrap();
        let stream_path = dir.path().join("test.stream.ndjson");
        let content = r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello from acpx"}}}}"#;
        fs::write(&stream_path, content).unwrap();

        let session = sessions::Session {
            acpx_record_id: "rec-1".into(),
            acp_session_id: "sess-1".into(),
            agent_type: "claude".into(),
            cwd: "/tmp".into(),
            status: sessions::SessionStatus::Exited,
            last_used_at: "2026-01-01T00:00:00Z".into(),
            stream_path: Some(stream_path.to_str().unwrap().to_string()),
            name: None,
        };

        let events = load_events_for(&session);
        assert_eq!(events.len(), 1);
        match &events[0] {
            crate::events::DisplayEvent::Message(text) => assert_eq!(text, "Hello from acpx"),
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_load_events_for_no_stream_no_openclaw() {
        let session = sessions::Session {
            acpx_record_id: "rec-1".into(),
            acp_session_id: "sess-1".into(),
            agent_type: "claude".into(),
            cwd: "/tmp".into(),
            status: sessions::SessionStatus::Exited,
            last_used_at: "2026-01-01T00:00:00Z".into(),
            stream_path: None,
            name: None,
        };

        let events = load_events_for(&session);
        assert!(events.is_empty());
    }

    #[test]
    fn test_load_events_for_empty_acpx_stream_falls_through() {
        let dir = tempfile::tempdir().unwrap();
        // Create an empty stream file (no parseable events)
        let stream_path = dir.path().join("empty.stream.ndjson");
        fs::write(&stream_path, "").unwrap();

        let session = sessions::Session {
            acpx_record_id: "rec-1".into(),
            acp_session_id: "sess-1".into(),
            agent_type: "codex".into(),
            cwd: "/tmp".into(),
            status: sessions::SessionStatus::Exited,
            last_used_at: "2026-01-01T00:00:00Z".into(),
            stream_path: Some(stream_path.to_str().unwrap().to_string()),
            name: Some("agent:codex:acp:fake-uuid".into()),
        };

        // Falls through acpx (empty), then openclaw resolve fails (no real openclaw dir)
        let events = load_events_for(&session);
        assert!(events.is_empty());
    }
}
