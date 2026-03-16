# Agent Registry 设计：扩展 acpx-tui 支持 15 个 Agent

> 增量设计文档，基于现有 acpx-tui 架构扩展全量 agent 支持。

## 背景

当前 acpx-tui 只支持 claude 和 codex 两个 agent 的识别和 resume。acpx 已内置 14 个 agent，加上本地已安装的 trae-cli，共需支持 15 个 agent。

### acpx 内置 agent 完整列表

| Agent | acpx 命令 | ACP 接入方式 |
|-------|----------|-------------|
| pi | `npx pi-acp` | 第三方 adapter |
| openclaw | `openclaw acp` | 原生 ACP bridge |
| codex | `npx @zed-industries/codex-acp` | Zed adapter |
| claude | `npx -y @zed-industries/claude-agent-acp` | Zed adapter |
| gemini | `gemini --acp` | 原生 `--acp` |
| cursor | `cursor-agent acp` | 原生子命令 |
| copilot | `copilot --acp --stdio` | 原生 `--acp` |
| droid | `droid exec --output-format acp` | 原生 ACP 输出 |
| iflow | `iflow --experimental-acp` | 实验性 ACP |
| kilocode | `npx -y @kilocode/cli acp` | npm ACP 模式 |
| kimi | `kimi acp` | 原生子命令 |
| kiro | `kiro-cli acp` | 原生子命令 |
| opencode | `npx -y opencode-ai acp` | npm ACP 模式 |
| qwen | `qwen --acp` | 原生 `--acp` |

### trae-cli（额外支持）

- 版本: 0.111.5
- 路径: `/Users/admin/.local/bin/trae-cli`
- ACP: `trae-cli acp serve`（原生支持）
- Resume: `trae-cli --resume <session_id>`

## 设计决策

**方案选择：Agent Registry 模式**

引入集中的静态注册表管理所有 agent 元数据，替代散落在多处的 match 分支。

选择理由：
- 新增 agent 只需加一行，零逻辑变更
- 名称、颜色、resume 命令集中管理，一致性强
- 比扩展 match 分支好维护，比配置文件驱动更简单

## 详细设计

### 1. 新增 `src/agents.rs` — Agent Registry

```rust
use ratatui::style::Color;

/// Resume 命令的模式
pub enum ResumePattern {
    /// `<binary> <flag> <session_id>`
    CliFlag { binary: &'static str, flag: &'static str },
    /// 暂不支持 resume
    Unsupported,
}

pub struct AgentInfo {
    pub name: &'static str,
    pub display_color: Color,
    pub resume: ResumePattern,
}

pub const AGENTS: &[AgentInfo] = &[
    AgentInfo { name: "pi",       display_color: Color::Green,          resume: ResumePattern::Unsupported },
    AgentInfo { name: "openclaw", display_color: Color::Blue,           resume: ResumePattern::Unsupported },
    AgentInfo { name: "codex",    display_color: Color::Cyan,           resume: ResumePattern::CliFlag { binary: "codex", flag: "--resume" } },
    AgentInfo { name: "claude",   display_color: Color::Magenta,        resume: ResumePattern::CliFlag { binary: "claude", flag: "--resume" } },
    AgentInfo { name: "trae",     display_color: Color::LightCyan,      resume: ResumePattern::CliFlag { binary: "trae-cli", flag: "--resume" } },
    AgentInfo { name: "gemini",   display_color: Color::Yellow,         resume: ResumePattern::Unsupported },
    AgentInfo { name: "cursor",   display_color: Color::LightGreen,     resume: ResumePattern::Unsupported },
    AgentInfo { name: "copilot",  display_color: Color::White,          resume: ResumePattern::Unsupported },
    AgentInfo { name: "droid",    display_color: Color::LightRed,       resume: ResumePattern::Unsupported },
    AgentInfo { name: "iflow",    display_color: Color::LightBlue,      resume: ResumePattern::Unsupported },
    AgentInfo { name: "kilocode", display_color: Color::LightYellow,    resume: ResumePattern::Unsupported },
    AgentInfo { name: "kimi",     display_color: Color::LightMagenta,   resume: ResumePattern::Unsupported },
    AgentInfo { name: "kiro",     display_color: Color::Red,            resume: ResumePattern::Unsupported },
    AgentInfo { name: "opencode", display_color: Color::Gray,           resume: ResumePattern::Unsupported },
    AgentInfo { name: "qwen",     display_color: Color::Rgb(255,165,0), resume: ResumePattern::Unsupported },
];

pub fn lookup(name: &str) -> Option<&'static AgentInfo> {
    AGENTS.iter().find(|a| a.name == name)
}

pub fn default_info() -> AgentInfo {
    AgentInfo {
        name: "unknown",
        display_color: Color::DarkGray,
        resume: ResumePattern::Unsupported,
    }
}
```

### 2. 修改 `src/sessions.rs` — agent 类型识别

扩展 `parse_agent_type()` 覆盖全部 15 个 agent：

```rust
fn parse_agent_type(agent_command: &str) -> String {
    let cmd_lower = agent_command.to_lowercase();
    for agent in agents::AGENTS {
        if cmd_lower.contains(agent.name) {
            return agent.name.to_string();
        }
    }
    // trae-cli 特殊处理：命令中是 "trae-cli" 或 "trae-agent"
    if cmd_lower.contains("trae") {
        return "trae".to_string();
    }
    agent_command.split_whitespace().last()
        .unwrap_or("unknown").to_string()
}
```

### 3. 修改 `src/resume.rs` — 查表替代 match

```rust
pub fn build_resume_command(session: &Session) -> Result<(String, Vec<String>), ResumeError> {
    let info = agents::lookup(&session.agent_type)
        .unwrap_or(&agents::default_info());
    match &info.resume {
        ResumePattern::CliFlag { binary, flag } => Ok((
            binary.to_string(),
            vec![flag.to_string(), session.acp_session_id.clone()],
        )),
        ResumePattern::Unsupported => Err(ResumeError::UnsupportedAgent(
            session.agent_type.clone(),
        )),
    }
}
```

### 4. 修改 `src/ui.rs` — 彩色 agent 标签

Session 列表中 agent 名称使用注册表中的颜色渲染：

```rust
fn render_session_line(session: &Session) -> Line<'_> {
    let info = agents::lookup(&session.agent_type)
        .unwrap_or(&agents::default_info());
    Line::from(vec![
        Span::styled(
            format!("[{}]", session.agent_type),
            Style::default().fg(info.display_color).bold(),
        ),
        Span::raw(" "),
        Span::raw(shorten_path(&session.cwd, 30)),
        Span::raw(" "),
        Span::styled(format_status(&session.status), status_style(&session.status)),
    ])
}
```

### 5. 修改 `src/app.rs` — Enter 键 unsupported 提示

新增 `status_message` 字段和 `set_status_message()` 方法。
按 Enter 时，对不支持 resume 的 agent 在状态栏显示提示：

```rust
KeyCode::Enter => {
    if let Some(session) = app.selected_session() {
        let info = agents::lookup(&session.agent_type)
            .unwrap_or(&agents::default_info());
        match &info.resume {
            ResumePattern::CliFlag { .. } => {
                exec_resume(session)?;
            }
            ResumePattern::Unsupported => {
                app.set_status_message(
                    format!("{} 暂不支持一键恢复", session.agent_type)
                );
            }
        }
    }
}
```

## 文件变更汇总

| 文件 | 操作 | 说明 |
|------|------|------|
| `src/agents.rs` | 新增 | Agent Registry 定义（15 个 agent） |
| `src/main.rs` | 修改 | 新增 `mod agents;` |
| `src/sessions.rs` | 修改 | `parse_agent_type()` 遍历 registry |
| `src/resume.rs` | 修改 | `build_resume_command()` 查表替代 match |
| `src/ui.rs` | 修改 | session 渲染使用彩色 agent 标签 |
| `src/app.rs` | 修改 | Enter 键增加 unsupported 提示 |

## Resume 支持状态

| Agent | Resume 状态 | 命令 |
|-------|------------|------|
| claude | 已支持 | `claude --resume <id>` |
| codex | 已支持 | `codex --resume <id>` |
| trae | 已支持 | `trae-cli --resume <id>` |
| 其余 12 个 | 暂不支持 | 逐个验证后添加 |

## 未来扩展

- 逐步为其他 agent 验证并添加 resume 支持
- `ResumePattern` 可扩展新的变体（如 WebSocket resume、ACP session/load）
- 颜色可按用户偏好覆盖（但当前阶段不做）
