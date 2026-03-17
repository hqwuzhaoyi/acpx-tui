use ratatui::style::Color;

/// Resume command pattern for an agent
pub enum ResumePattern {
    /// `<binary> <flag> <session_id>` — agent supports resume
    CliFlag {
        binary: &'static str,
        flag: &'static str,
    },
    /// Agent does not yet support resume
    Unsupported,
}

/// Metadata for a known agent
pub struct AgentInfo {
    pub name: &'static str,
    pub display_color: Color,
    pub resume: ResumePattern,
}

/// Static registry of all 15 supported agents
///
/// NOTE: Ordering matters for `parse_agent_type` substring matching in sessions.rs.
/// Agents whose names are substrings of other agent names (e.g. "pi" in "copilot")
/// MUST come after the longer name to avoid false matches.
pub const AGENTS: &[AgentInfo] = &[
    AgentInfo { name: "openclaw", display_color: Color::Blue,             resume: ResumePattern::Unsupported },
    AgentInfo { name: "codex",    display_color: Color::Cyan,             resume: ResumePattern::CliFlag { binary: "codex", flag: "resume" } },
    AgentInfo { name: "claude",   display_color: Color::Magenta,          resume: ResumePattern::CliFlag { binary: "claude", flag: "--resume" } },
    AgentInfo { name: "trae",     display_color: Color::LightCyan,        resume: ResumePattern::CliFlag { binary: "trae-cli", flag: "--resume" } },
    AgentInfo { name: "gemini",   display_color: Color::Yellow,           resume: ResumePattern::Unsupported },
    AgentInfo { name: "cursor",   display_color: Color::LightGreen,       resume: ResumePattern::Unsupported },
    AgentInfo { name: "copilot",  display_color: Color::White,            resume: ResumePattern::Unsupported },
    AgentInfo { name: "droid",    display_color: Color::LightRed,         resume: ResumePattern::Unsupported },
    AgentInfo { name: "iflow",    display_color: Color::LightBlue,        resume: ResumePattern::Unsupported },
    AgentInfo { name: "kilocode", display_color: Color::LightYellow,      resume: ResumePattern::Unsupported },
    AgentInfo { name: "kimi",     display_color: Color::LightMagenta,     resume: ResumePattern::Unsupported },
    AgentInfo { name: "kiro",     display_color: Color::Red,              resume: ResumePattern::Unsupported },
    AgentInfo { name: "opencode", display_color: Color::Gray,             resume: ResumePattern::Unsupported },
    AgentInfo { name: "qwen",     display_color: Color::Rgb(255, 165, 0), resume: ResumePattern::Unsupported },
    AgentInfo { name: "pi",       display_color: Color::Green,            resume: ResumePattern::Unsupported },
];

/// Look up agent info by name
pub fn lookup(name: &str) -> Option<&'static AgentInfo> {
    AGENTS.iter().find(|a| a.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_15_agents() {
        assert_eq!(AGENTS.len(), 15);
    }

    #[test]
    fn test_lookup_claude() {
        let info = lookup("claude").unwrap();
        assert_eq!(info.name, "claude");
        assert!(matches!(info.resume, ResumePattern::CliFlag { binary: "claude", .. }));
    }

    #[test]
    fn test_lookup_codex() {
        let info = lookup("codex").unwrap();
        assert_eq!(info.name, "codex");
        assert!(matches!(info.resume, ResumePattern::CliFlag { binary: "codex", .. }));
    }

    #[test]
    fn test_lookup_trae() {
        let info = lookup("trae").unwrap();
        assert_eq!(info.name, "trae");
        assert!(matches!(info.resume, ResumePattern::CliFlag { binary: "trae-cli", .. }));
    }

    #[test]
    fn test_lookup_unsupported_agent_has_unsupported_resume() {
        let info = lookup("gemini").unwrap();
        assert!(matches!(info.resume, ResumePattern::Unsupported));
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        assert!(lookup("nonexistent").is_none());
    }

    #[test]
    fn test_all_agents_have_unique_names() {
        let mut names: Vec<&str> = AGENTS.iter().map(|a| a.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), AGENTS.len());
    }

    #[test]
    fn test_resume_agents_count() {
        let resumable: Vec<_> = AGENTS.iter().filter(|a| matches!(a.resume, ResumePattern::CliFlag { .. })).collect();
        assert_eq!(resumable.len(), 3); // claude, codex, trae
    }
}
