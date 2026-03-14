# acpx-tui 设计文档

> 2026-03-14

## 定位

acpx session 的实时仪表盘，选中后一键 resume 到完整终端 TUI。

## 背景

OpenClaw + acpx 已覆盖 agent 通信、通知、会话管理等功能。经测试验证，acpx 创建的 Claude Code session 可以直接用 `claude --resume <acp_session_id>` 恢复到完整 TUI。

acpx-tui 是一个独立项目（非 CAM 子模块），只做一件事：让用户看到所有 acpx session 的实时状态，选中后一键进入终端手动操作。

## 技术栈

- Rust + ratatui（TUI 框架）
- 独立 repo，独立二进制

## 架构

```
~/.acpx/sessions/
  ├── index.json              ← 读取 session 列表
  ├── <id>.json               ← 读取 session 元数据
  └── <id>.stream.ndjson      ← tail 读取实时事件流

acpx-tui：
  1. 解析 index.json → session 列表
  2. 对每个 session 读 .json → 状态（running/exited/closed）
  3. 选中的 session → tail .stream.ndjson → 右侧事件流面板
  4. 按 Enter → exec claude --resume <acp_session_id>
     → acpx-tui 进程被替换，用户进入 agent 完整 TUI
```

## TUI 布局

```
┌─ acpx-tui ────────────────────────────────────────┐
│ Sessions                 │ Events                  │
│                          │                          │
│ ● claude  ~/project-a    │ 🔧 tool_call: Bash      │
│   5m ago · running       │    echo "hello world"    │
│                          │ 💬 agent_message:        │
│ ○ codex   ~/project-b   │    "Done. PR ready."    │
│   2h ago · exited        │                          │
│                          │ 🔧 tool_call: Edit       │
│                          │    src/main.rs           │
│                          │                          │
├──────────────────────────┴──────────────────────────┤
│ [Enter] Resume  [d] Details  [r] Refresh  [q] Quit │
└─────────────────────────────────────────────────────┘
```

### 左面板：Sessions

- 从 `index.json` 读取 session 列表
- 显示：agent 类型、cwd（缩短）、时间、状态
- 状态判断：
  - `closed: true` → closed
  - `pid` 存在且 `kill(pid, 0)` 成功 → running
  - `last_agent_exit_at` 有值 → exited
- 定时刷新（2-3 秒轮询 index.json）

### 右面板：Events

- 选中 session 后 tail 其 `.stream.ndjson`
- 解析 ACP JSON-RPC 事件，简化显示：
  - `agent_message_chunk` → 消息文本
  - `tool_call` / `tool_call_update` → 工具名 + 标题
  - `agent_thought_chunk` → thinking 内容
  - `usage_update` → 费用信息
  - 其他事件 → 跳过

### 底部：快捷键

- `Enter` — Resume 选中的 session
- `d` — 显示 session 详情（JSON 元数据）
- `r` — 强制刷新
- `q` — 退出
- `j/k` 或 `↑/↓` — 导航

## Resume 行为

选中 session 按 Enter：

1. ratatui cleanup（恢复终端状态）
2. 根据 agent 类型构建命令：
   - `agent_command` 包含 `claude` → `claude --resume <acp_session_id>`
   - `agent_command` 包含 `codex` → `codex --resume <acp_session_id>`（待验证）
   - 其他 → 提示不支持 resume
3. `std::os::unix::process::CommandExt::exec()` 替换当前进程
4. 用户直接进入 agent 完整 TUI，acpx-tui 不再存在

## 数据源

| 数据 | 来源 | 方式 |
|------|------|------|
| Session 列表 | `~/.acpx/sessions/index.json` | 定时轮询 |
| Session 状态 | `<id>.json` 字段 | 读取 JSON |
| 进程存活 | `kill(pid, 0)` | 系统调用 |
| 事件流 | `<id>.stream.ndjson` | tail（seek to end） |

## 项目结构

```
acpx-tui/
  Cargo.toml
  src/
    main.rs           # CLI 入口 + TUI 启动
    app.rs            # App 状态管理
    sessions.rs       # 读取 ~/.acpx/sessions/
    events.rs         # 解析 .stream.ndjson
    ui.rs             # ratatui 布局渲染
    resume.rs         # exec resume 逻辑
```

## 依赖

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
```

## 不做的事

- 通知/webhook — OpenClaw thread 处理
- 智能审批 — 未来单独处理
- Agent 启动 — 用 acpx 命令
- Agent Teams — OpenClaw 处理
- AI 解析终端 — 不需要，事件是结构化的
- 会话管理 — acpx 自己管理

## 验证记录

2026-03-14 测试结果：

```
# acpx 创建 Claude Code session
acpx claude 'echo hello world' --approve-all --format json
→ acp_session_id: 4ed50f0f-8a1d-41ec-a1ce-a59751baa957
→ sessionCapabilities: { fork, list, resume }

# claude --resume 成功恢复
claude --resume 4ed50f0f-8a1d-41ec-a1ce-a59751baa957 -p 'say hello'
→ "Hello! This is a resume test — session resumed successfully."
```
