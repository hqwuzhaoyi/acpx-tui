use crate::sessions::Session;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const CREATE_SESSION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptResult {
    pub stdout: String,
    pub stderr: String,
}

impl PromptResult {
    pub fn summary(&self) -> String {
        let stdout = self.stdout.trim();
        let stderr = self.stderr.trim();

        if !stdout.is_empty() {
            format!("Prompt sent: {}", first_line(stdout))
        } else if !stderr.is_empty() {
            format!("Prompt sent: {}", first_line(stderr))
        } else {
            "Prompt sent".to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSessionResult {
    pub stdout: String,
    pub stderr: String,
}

impl CreateSessionResult {
    pub fn summary(&self, agent: &str) -> String {
        let stdout = self.stdout.trim();
        let stderr = self.stderr.trim();

        if !stdout.is_empty() {
            format!("Created {} session: {}", agent, first_line(stdout))
        } else if !stderr.is_empty() {
            format!("Created {} session: {}", agent, first_line(stderr))
        } else {
            format!("Created {} session", agent)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpxControlError {
    EmptyPrompt,
    AgentDiscoveryFailed(String),
    InvalidAgentName(String),
    SpawnFailed(String),
    CommandTimedOut {
        timeout_seconds: u64,
    },
    CommandFailed {
        status: String,
        stdout: String,
        stderr: String,
    },
}

impl std::fmt::Display for AcpxControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcpxControlError::EmptyPrompt => write!(f, "prompt is empty"),
            AcpxControlError::AgentDiscoveryFailed(message) => {
                write!(f, "failed to discover acpx agents: {}", message)
            }
            AcpxControlError::InvalidAgentName(agent) => {
                write!(f, "invalid acpx agent name: {}", agent)
            }
            AcpxControlError::SpawnFailed(message) => write!(f, "failed to run acpx: {}", message),
            AcpxControlError::CommandTimedOut { timeout_seconds } => {
                write!(f, "acpx command timed out after {}s", timeout_seconds)
            }
            AcpxControlError::CommandFailed {
                status,
                stdout,
                stderr,
            } => {
                let details = if !stderr.trim().is_empty() {
                    stderr.trim()
                } else if !stdout.trim().is_empty() {
                    stdout.trim()
                } else {
                    "no output"
                };
                write!(f, "acpx exited with {}: {}", status, first_line(details))
            }
        }
    }
}

const TOP_LEVEL_NON_AGENT_COMMANDS: &[&str] = &[
    "prompt", "exec", "cancel", "set-mode", "set", "status", "sessions", "config",
];

/// Pick the most stable TUI key for prompt history and display.
///
/// Named sessions are accepted by acpx's `-s` prompt path. Unnamed sessions are
/// addressed by running acpx in the session cwd without `-s`, but the ACP session id
/// is still useful as a stable local key.
pub fn prompt_session_selector(session: &Session) -> String {
    session
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&session.acp_session_id)
        .to_string()
}

pub fn build_prompt_args(session: &Session, prompt: &str) -> Vec<String> {
    let mut args = vec![session.agent_type.clone(), "prompt".to_string()];

    if let Some(name) = session
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        args.push("-s".to_string());
        args.push(name.to_string());
    }

    args.push(prompt.to_string());
    args
}

pub fn parse_registered_agents_from_help(help: &str) -> Vec<String> {
    let mut agents = Vec::new();

    for line in help.lines() {
        let trimmed = line.trim_start();
        let Some((candidate, _rest)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        if TOP_LEVEL_NON_AGENT_COMMANDS.contains(&candidate) {
            continue;
        }
        if is_safe_agent_name(candidate) && trimmed.contains(&format!("Use {} agent", candidate)) {
            agents.push(candidate.to_string());
        }
    }

    agents.sort();
    agents.dedup();
    agents
}

pub fn discover_registered_agents() -> Result<Vec<String>, AcpxControlError> {
    let invocation = resolve_acpx_command().map_err(AcpxControlError::SpawnFailed)?;
    let output = run_acpx_with_timeout(
        &invocation,
        vec!["--help".to_string()],
        None,
        Duration::from_secs(10),
    )?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(AcpxControlError::CommandFailed {
            status: output.status.to_string(),
            stdout,
            stderr,
        });
    }

    let agents = parse_registered_agents_from_help(&stdout);
    if agents.is_empty() {
        Err(AcpxControlError::AgentDiscoveryFailed(
            "acpx --help did not include any registered agent commands".to_string(),
        ))
    } else {
        Ok(agents)
    }
}

pub fn build_create_session_args(agent: &str, cwd: &Path) -> Vec<String> {
    vec![
        "--cwd".to_string(),
        cwd.display().to_string(),
        agent.to_string(),
        "sessions".to_string(),
        "new".to_string(),
    ]
}

pub fn create_session(agent: &str, cwd: &Path) -> Result<CreateSessionResult, AcpxControlError> {
    validate_agent_name(agent)?;
    let invocation = resolve_acpx_command().map_err(AcpxControlError::SpawnFailed)?;
    let output = run_acpx_with_timeout(
        &invocation,
        build_create_session_args(agent, cwd),
        Some(cwd),
        CREATE_SESSION_TIMEOUT,
    )?;

    let result = CreateSessionResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };

    if output.status.success() {
        Ok(result)
    } else {
        Err(AcpxControlError::CommandFailed {
            status: output.status.to_string(),
            stdout: result.stdout,
            stderr: result.stderr,
        })
    }
}

fn validate_agent_name(agent: &str) -> Result<(), AcpxControlError> {
    if is_safe_agent_name(agent) {
        Ok(())
    } else {
        Err(AcpxControlError::InvalidAgentName(agent.to_string()))
    }
}

fn is_safe_agent_name(agent: &str) -> bool {
    !agent.is_empty()
        && !agent.starts_with('-')
        && agent
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn run_acpx_with_timeout(
    invocation: &AcpxInvocation,
    args: Vec<String>,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<Output, AcpxControlError> {
    let mut command = Command::new(&invocation.program);
    command
        .args(&invocation.prefix_args)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("ANTHROPIC_BASE_URL");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let mut child = command.spawn().map_err(|e| {
        AcpxControlError::SpawnFailed(format!("{} ({})", e, invocation.description))
    })?;
    let started = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child.wait_with_output().map_err(|e| {
                    AcpxControlError::SpawnFailed(format!(
                        "failed to read acpx output: {} ({})",
                        e, invocation.description
                    ))
                });
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AcpxControlError::CommandTimedOut {
                    timeout_seconds: timeout.as_secs(),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AcpxControlError::SpawnFailed(format!(
                    "failed to poll acpx process: {} ({})",
                    e, invocation.description
                )));
            }
        }
    }
}

pub fn send_prompt(session: &Session, prompt: &str) -> Result<PromptResult, AcpxControlError> {
    if prompt.trim().is_empty() {
        return Err(AcpxControlError::EmptyPrompt);
    }
    validate_agent_name(&session.agent_type)?;

    let invocation = resolve_acpx_command().map_err(AcpxControlError::SpawnFailed)?;
    let output = Command::new(&invocation.program)
        .args(&invocation.prefix_args)
        .args(build_prompt_args(session, prompt))
        .current_dir(&session.cwd)
        // Keep behavior aligned with CAM's ACP transport: avoid leaking alternate
        // Anthropic endpoints into agent subprocesses unless acpx itself injects them.
        .env_remove("ANTHROPIC_BASE_URL")
        .output()
        .map_err(|e| {
            AcpxControlError::SpawnFailed(format!("{} ({})", e, invocation.description))
        })?;

    let result = PromptResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    };

    if output.status.success() {
        Ok(result)
    } else {
        Err(AcpxControlError::CommandFailed {
            status: output.status.to_string(),
            stdout: result.stdout,
            stderr: result.stderr,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AcpxInvocation {
    program: String,
    prefix_args: Vec<String>,
    description: String,
}

fn resolve_acpx_command() -> Result<AcpxInvocation, String> {
    if let Ok(binary) = std::env::var("ACPX_BIN") {
        let binary = binary.trim();
        if !binary.is_empty() {
            return Ok(AcpxInvocation {
                program: binary.to_string(),
                prefix_args: vec![],
                description: format!("ACPX_BIN={}", binary),
            });
        }
    }

    if let Some(path) = find_executable("acpx") {
        return Ok(direct_invocation(path, "PATH"));
    }

    for candidate in common_acpx_paths() {
        if is_executable(&candidate) {
            return Ok(direct_invocation(
                candidate.to_path_buf(),
                "common install path",
            ));
        }
    }

    if let Some(npx) = find_executable("npx") {
        return Ok(AcpxInvocation {
            program: npx.to_string_lossy().to_string(),
            prefix_args: vec!["-y".to_string(), "acpx".to_string()],
            description: "npx -y acpx fallback".to_string(),
        });
    }

    Err(format!(
        "could not find acpx. Set ACPX_BIN=/absolute/path/to/acpx, or ensure PATH includes one of: {}",
        common_acpx_paths()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn direct_invocation(path: PathBuf, source: &str) -> AcpxInvocation {
    AcpxInvocation {
        program: path.to_string_lossy().to_string(),
        prefix_args: vec![],
        description: format!("{} ({})", path.display(), source),
    }
}

fn common_acpx_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/opt/homebrew/bin/acpx"),
        PathBuf::from("/usr/local/bin/acpx"),
        PathBuf::from("/usr/bin/acpx"),
        PathBuf::from("/bin/acpx"),
    ]
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().count() > 1 {
        return is_executable(candidate).then(|| candidate.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::{Session, SessionStatus};

    fn make_session(name: Option<&str>) -> Session {
        Session {
            acpx_record_id: "record-1".to_string(),
            acp_session_id: "acp-123".to_string(),
            agent_type: "codex".to_string(),
            cwd: "/tmp".to_string(),
            status: SessionStatus::Running,
            last_used_at: "2026-04-28T00:00:00Z".to_string(),
            stream_path: None,
            name: name.map(str::to_string),
        }
    }

    #[test]
    fn prompt_selector_prefers_named_session() {
        let session = make_session(Some("agent:codex:acp:abc"));
        assert_eq!(prompt_session_selector(&session), "agent:codex:acp:abc");
    }

    #[test]
    fn prompt_selector_falls_back_to_acp_session_id() {
        let session = make_session(None);
        assert_eq!(prompt_session_selector(&session), "acp-123");
    }

    #[test]
    fn build_prompt_args_matches_acpx_prompt_shape() {
        let session = make_session(Some("agent:codex:acp:abc"));
        assert_eq!(
            build_prompt_args(&session, "hello"),
            vec!["codex", "prompt", "-s", "agent:codex:acp:abc", "hello"]
        );
    }

    #[test]
    fn build_prompt_args_uses_cwd_default_for_unnamed_sessions() {
        let session = make_session(None);
        assert_eq!(
            build_prompt_args(&session, "hello"),
            vec!["codex", "prompt", "hello"]
        );
    }

    #[test]
    fn parse_registered_agents_reads_acpx_help_agent_commands() {
        let help = r#"
Commands:
  pi [options] [prompt...]                Use pi agent
  codex [options] [prompt...]             Use codex agent
  hermes [options] [prompt...]            Use hermes agent
  prompt [options] [prompt...]            Prompt using codex by default
  sessions                                List, ensure, create, or close sessions for this agent
  config                                  Inspect and initialize acpx configuration
"#;
        assert_eq!(
            parse_registered_agents_from_help(help),
            vec!["codex".to_string(), "hermes".to_string(), "pi".to_string()]
        );
    }

    #[test]
    fn parse_registered_agents_rejects_option_shaped_and_unsafe_names() {
        let help = r#"
Commands:
  --cwd [options] [prompt...]              Use --cwd agent
  ../evil [options] [prompt...]            Use ../evil agent
  bad/name [options] [prompt...]           Use bad/name agent
  bad$name [options] [prompt...]           Use bad$name agent
  good-agent [options] [prompt...]         Use good-agent agent
  good_agent [options] [prompt...]         Use good_agent agent
"#;
        assert_eq!(
            parse_registered_agents_from_help(help),
            vec!["good-agent".to_string(), "good_agent".to_string()]
        );
    }

    #[test]
    fn validate_agent_name_blocks_option_injection() {
        assert!(validate_agent_name("codex").is_ok());
        assert!(validate_agent_name("good-agent_2").is_ok());
        assert_eq!(
            validate_agent_name("--cwd").unwrap_err(),
            AcpxControlError::InvalidAgentName("--cwd".to_string())
        );
        assert!(validate_agent_name("bad/name").is_err());
        assert!(validate_agent_name("bad name").is_err());
        assert!(validate_agent_name("").is_err());
    }

    #[test]
    fn build_create_session_args_matches_acpx_sessions_new_shape() {
        assert_eq!(
            build_create_session_args("codex", Path::new("/tmp/project")),
            vec!["--cwd", "/tmp/project", "codex", "sessions", "new"]
        );
    }

    #[test]
    fn direct_invocation_has_no_prefix_args() {
        let invocation = direct_invocation(PathBuf::from("/tmp/acpx"), "test");
        assert_eq!(invocation.program, "/tmp/acpx");
        assert!(invocation.prefix_args.is_empty());
        assert!(invocation.description.contains("test"));
    }

    #[test]
    fn prompt_result_summary_prefers_stdout() {
        let result = PromptResult {
            stdout: "done\nmore".to_string(),
            stderr: "warning".to_string(),
        };
        assert_eq!(result.summary(), "Prompt sent: done");
    }

    #[test]
    fn empty_prompt_is_rejected_before_spawn() {
        let session = make_session(None);
        assert_eq!(
            send_prompt(&session, "  ").unwrap_err(),
            AcpxControlError::EmptyPrompt
        );
    }

    #[test]
    fn send_prompt_rejects_unsafe_agent_name_before_spawn() {
        let mut session = make_session(None);
        session.agent_type = "--cwd".to_string();
        assert_eq!(
            send_prompt(&session, "hello").unwrap_err(),
            AcpxControlError::InvalidAgentName("--cwd".to_string())
        );
    }
}
