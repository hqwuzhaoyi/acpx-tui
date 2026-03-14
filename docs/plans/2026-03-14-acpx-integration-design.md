# CAM + acpx 集成设计

> 2026-03-14 brainstorming 研究结论

## 背景

CAM (Code Agent Monitor) 和 acpx (Agent Client Protocol CLI) 的能力对比研究，明确 CAM 在 OpenClaw + acpx 生态中的定位和演进方向。

## 研究结论

### acpx 是什么

acpx 是 OpenClaw 官方的 headless CLI 客户端，实现 Agent Client Protocol (ACP)，用结构化 JSON-RPC 替代 PTY scraping 与 coding agent 通信。

- **已内置 15+ agent**：Claude、Codex、Gemini、OpenClaw、OpenCode 等
- **核心能力**：会话管理、prompt 队列、命名并行会话、crash 重连
- **存储**：`~/.acpx/sessions/` (JSON checkpoint + .stream.ndjson 事件日志)
- **GitHub**：https://github.com/openclaw/acpx

### OpenClaw + acpx 官方集成现状

OpenClaw 已有完整的 acpx 插件集成：

```bash
openclaw plugins install acpx
openclaw config set acp.enabled true
openclaw config set acp.backend acpx
openclaw config set acp.defaultAgent claude
```

**官方流程**：
```
用户 (Discord/Telegram)
  ↓ 发消息
OpenClaw → acp-router skill → /acp spawn claude
  ↓
acpx 启动 Claude Code (ACP 子进程)
  ↓ 工作完成
结果自动出现在 thread 里
  ↓
用户在 thread 里继续对话 → 自动路由到同一 session
```

**thread 绑定**：Discord thread / Telegram topic 自动绑定到 ACP session，实现持久化多轮对话。

### CAM 功能被 acpx 官方路径替代的部分

| CAM 功能 | 官方路径替代方案 | 状态 |
|----------|-----------------|------|
| 远程通知 agent 在问什么 | thread 绑定，agent 输出自动出现 | ✅ 已解决 |
| 远程回复 agent 的问题 | thread 里直接回复 | ✅ 已解决 |
| 检测 agent 等待输入 | agent turn 结束 = 等待 | ✅ 已解决 |
| AI 解析终端提取问题 | 结构化文本输出，不需要 AI | ✅ 已解决 |
| tmux send-keys 发送回复 | acpx prompt queue | ✅ 已解决 |
| 启动 agent | acpx spawn | ✅ 已解决 |
| 会话管理 | acpx sessions | ✅ 已解决 |
| 多 agent 支持 | 内置 15+ agent | ✅ 更广 |

### 官方路径仍有的缺口

| 缺口 | 说明 |
|------|------|
| 权限请求只有 approve-all | ACP bridge 不实现 `session/request_permission`，权限请求从不路由到用户 |
| Agent 卡住/空转无检测 | acpx 不做主动监控 |
| 智能权限审批 | 无白名单/黑名单/LLM 三层决策 |
| Agent Teams 编排 | acpx 明确排除 |
| 从 ACP session 恢复到终端 TUI | 无官方方案 |

### CAM 重新定位

**CAM 从 "监控 + 通知 + 管理" 缩小为以下核心场景：**

OpenClaw 从需求开始分析，通过 acpx 使用多个 agent 进行编码和拆解。CAM 提供：

1. **Skill 层**：识别 agent 状态，给用户建议（何时需要人工介入、何时可以自动处理）
2. **快速恢复**：从 acpx headless session 恢复到终端 TUI，手动操作 Claude Code 或其他 agent

### 技术关键点

#### 权限请求的完整生命周期

```
Claude Code → client/requestPermission → acpx → resolvePermissionRequest()

四种模式：
- approve-all：全部自动批准（官方推荐的非交互模式）
- approve-reads：读自动批准，写/执行需交互（默认）
- deny-all：全部拒绝
- 交互模式：TTY prompt（需要人在终端前）

缺失的第五种模式：
- permission hook：调用外部命令处理 → 这是 CAM 智能审批的理想插入点
```

#### acpx 可消费的事件源

```
                能观察事件？  能拦截权限？  能发送回复？
stdout NDJSON       ✅           ❌           ❌
.stream.ndjson      ✅           ❌           ❌
Queue Socket        ❌           ❌           ✅
MCP Server          ❌           ❌      ✅（agent 主动调用）
permission hook     ✅           ✅           ✅  ← 需贡献给 acpx
```

#### claude --resume 兼容性（待验证）

acpx 通过 `claude-agent-acp` 启动 Claude Code 时创建的 session，可能可以用 `claude --resume <session_id>` 恢复到完整 TUI。需要验证：
- agent_session_id 是否与 claude --resume 兼容
- 对话历史是否完整保留
- 两种模式能否来回切换

**待办**：见 Task #7

## 未来方向

### 短期：CAM 作为 OpenClaw Skill

CAM 缩减为 OpenClaw skill，提供：
- Agent 状态识别和建议
- 快速从 acpx session 恢复到终端 TUI 的能力
- 多 agent 会话的概览和管理

### 中期：向 acpx 贡献 permission hook

设计一个 `--on-permission` 参数，让外部进程可以处理权限请求：

```bash
acpx claude prompt "..." --on-permission "cam approve-check"
```

权限请求到达时：
1. acpx 调用外部命令，传入请求 JSON
2. 外部命令（CAM/OpenClaw）评估风险
3. 返回 approve/deny
4. acpx 把结果发回 agent

### 长期：完整的安全审批链路

```
Agent → 权限请求 → acpx permission hook → CAM/OpenClaw 三层审批
  → 低风险：自动批准
  → 中风险：通知用户，等待回复
  → 高风险：拒绝并建议替代方案
```

## 参考资料

- [acpx GitHub](https://github.com/openclaw/acpx)
- [OpenClaw ACP Agents 文档](https://docs.openclaw.ai/tools/acp-agents)
- [OpenClaw ACP CLI 文档](https://docs.openclaw.ai/cli/acp)
- [OpenClaw ACP Thread Bound Agents 设计](https://docs.openclaw.ai/experiments/plans/acp-thread-bound-agents)
- [ACP 协议缺口分析](https://shashikantjagtap.net/openclaw-acp-what-coding-agent-users-need-to-know-about-protocol-gaps/)
- [ACP 防止 Agent Hang](https://www.bighatgroup.com/blog/using-acp-with-openclaw-to-prevent-agent-hangs/)
- [GitHub Issue #28511 — 标准 ACP 支持提案](https://github.com/openclaw/openclaw/issues/28511)
- [acp-router skill (Lobehub)](https://lobehub.com/skills/openclaw-openclaw-acp-router)
