# Plan: 支持 OpenClaw 事件流显示

## 问题

当 openclaw 通过 acpx 分派子 agent（如 codex）时，实际对话事件记录在 openclaw 自己的 stream 文件中，而非 acpx 的 stream 文件。导致 TUI 的 Events 面板显示空白。

## 数据流向

```
openclaw (主 agent, 飞书/Telegram 等渠道)
  └─ 通过 acpx 分派 codex/claude/trae 子 agent
       ├─ acpx stream (~/.acpx/sessions/<id>.stream.ndjson)
       │   → 只有 ACP 协议握手 (initialize, session/load, session/new)
       │   → 无对话内容
       └─ openclaw stream (~/.openclaw/agents/<agent>/sessions/<oc_id>.acp-stream.jsonl)
           → 完整对话: assistant_delta, system_event, lifecycle
           → 229+ 行实际事件
```

## 关键关联

- acpx session 的 `name` 字段（如 `agent:codex:acp:e68f428a-...`）对应 openclaw 的 session key
- openclaw 的 `sessions.json` 中通过 `acpxRecordId` 关联回 acpx session
- openclaw stream 路径: `~/.openclaw/agents/<agent>/sessions/<oc_session_id>.acp-stream.jsonl`

## openclaw 事件格式

与 acpx 的 ACP JSON-RPC 格式不同：

```jsonl
{"ts":"...","kind":"system_event","text":"Started codex session...","contextKey":"acp-spawn:..."}
{"ts":"...","kind":"lifecycle","phase":"start","data":{"phase":"start"}}
{"ts":"...","kind":"assistant_delta","delta":"我"}
{"ts":"...","kind":"assistant_delta","delta":"先"}
{"ts":"...","kind":"system_event","text":"codex: 我先读取相关说明...","contextKey":"acp-spawn:..."}
```

事件类型：
- `assistant_delta` — 流式文本片段（`delta` 字段），需要合并
- `system_event` — 系统消息（`text` 字段），包含 agent 思考过程摘要
- `lifecycle` — 生命周期事件（`phase`: start/end）

## 实现步骤

### Step 1: 解析 openclaw session 关联

在 `sessions.rs` 中：

1. 检测 acpx session 的 `name` 是否匹配 `agent:<agent>:acp:<uuid>` 模式
2. 如果匹配，读取 `~/.openclaw/agents/<agent>/sessions/sessions.json`
3. 通过 session key 查找对应的 `sessionId`
4. 构建 openclaw stream 路径: `~/.openclaw/agents/<agent>/sessions/<sessionId>.acp-stream.jsonl`

### Step 2: 解析 openclaw 事件格式

在 `events.rs` 中：

1. 新增 `parse_openclaw_event(line)` 函数，解析 `kind`/`delta`/`text` 格式
2. 新增 `load_openclaw_events(path, max_events)` 函数
3. 合并 `assistant_delta` 为完整消息（类似现有的 `merge_events`）
4. 将 `system_event` 映射为 `DisplayEvent::Message`

### Step 3: 事件加载回退逻辑

在 `app.rs` / `events.rs` 中：

1. 优先加载 acpx stream（现有逻辑）
2. 如果 acpx stream 为空或只有握手事件，尝试加载 openclaw stream
3. 合并显示

### Step 4: 测试

1. 为 openclaw 事件格式添加解析测试
2. 为 session 关联查找添加测试
3. 为回退逻辑添加测试

## 涉及文件

- `src/sessions.rs` — 添加 openclaw session 关联解析
- `src/events.rs` — 添加 openclaw 事件格式解析
- `src/app.rs` — 事件加载回退逻辑

## 注意事项

- openclaw 不一定安装，需要优雅降级（目录不存在时跳过）
- `sessions.json` 可能很大（1471 个 session），考虑性能
- openclaw stream 文件名用的是 openclaw 自己的 session ID，不是 acpx 的
- `assistant_delta` 按字/词粒度拆分，需要合并才能阅读
