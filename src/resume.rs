use crate::sessions::Session;
use std::os::unix::process::CommandExt;
use std::process::Command;

#[derive(Debug)]
pub enum ResumeError {
    UnsupportedAgent(String),
}

impl std::fmt::Display for ResumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResumeError::UnsupportedAgent(agent) => {
                write!(f, "Resume not supported for agent: {}", agent)
            }
        }
    }
}

/// Build the resume command for a session.
/// Returns (program, args) or error.
pub fn build_resume_command(session: &Session) -> Result<(String, Vec<String>), ResumeError> {
    match session.agent_type.as_str() {
        "claude" => Ok((
            "claude".to_string(),
            vec!["--resume".to_string(), session.acp_session_id.clone()],
        )),
        "codex" => Ok((
            "codex".to_string(),
            vec!["resume".to_string(), session.acp_session_id.clone()],
        )),
        other => Err(ResumeError::UnsupportedAgent(other.to_string())),
    }
}

/// Exec into the agent TUI, replacing the current process.
/// Changes to the session's cwd first, since agents look up sessions by project directory.
/// This function does not return on success.
pub fn exec_resume(session: &Session) -> Result<(), ResumeError> {
    let (program, args) = build_resume_command(session)?;

    // Agent CLIs resolve sessions by cwd, so we must chdir to the session's project directory
    if let Err(e) = std::env::set_current_dir(&session.cwd) {
        eprintln!("Warning: failed to chdir to {}: {}", session.cwd, e);
    }

    let err = Command::new(&program).args(&args).exec();

    // exec() only returns on error
    eprintln!("Failed to exec {} --resume: {}", program, err);
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::{Session, SessionStatus};

    fn make_session(agent_type: &str, session_id: &str) -> Session {
        Session {
            acpx_record_id: "rec-1".to_string(),
            acp_session_id: session_id.to_string(),
            agent_type: agent_type.to_string(),
            cwd: "/tmp".to_string(),
            status: SessionStatus::Exited,
            last_used_at: "2026-01-01T00:00:00Z".to_string(),
            stream_path: None,
        }
    }

    #[test]
    fn test_build_resume_command_claude() {
        let session = make_session("claude", "abc-123");
        let (prog, args) = build_resume_command(&session).unwrap();
        assert_eq!(prog, "claude");
        assert_eq!(args, vec!["--resume", "abc-123"]);
    }

    #[test]
    fn test_build_resume_command_codex() {
        let session = make_session("codex", "def-456");
        let (prog, args) = build_resume_command(&session).unwrap();
        assert_eq!(prog, "codex");
        assert_eq!(args, vec!["resume", "def-456"]);
    }

    #[test]
    fn test_build_resume_command_unsupported() {
        let session = make_session("unknown-agent", "xyz");
        let result = build_resume_command(&session);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            format!("{}", err),
            "Resume not supported for agent: unknown-agent"
        );
    }
}
