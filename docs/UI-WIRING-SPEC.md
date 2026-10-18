# UI 接线规格 — MCode_Desktop M2

> 执行者注意：本仓库历史上有"长内容生成损坏"问题。写完每个文件后必须 `cargo check -p mcode-ui 2>&1 | head -50` 验证，
> 发现损坏（奇怪的非 ASCII 标识符、半截代码、marker 字样）立即修复，不要相信"写入了 = 写对了"。

## 目标

mcode-ui（GPUI 桌面端）从静态原型升级为全接线：真实流式回合、权限弹窗真裁决、命令执行、
会话持久化、四视图。布局规范 = docs/prototype/screens/*.html（对照渲染）。

## 已就绪的内核 API（crates/mcode-core，36/36 测试绿，勿改内核语义）

- `runtime.rs` 已写好：UiRuntime(tokio 线程) / Harness{deps, ui_tx, ui_rx, mcp_report} / build_harness / event_sink。
  需要小改：`Harness.ui_rx` 要能被 App 接管——把 `ui_rx: tokio::sync::mpsc::UnboundedReceiver<UiEvent>` 从字段改为
  `rx_slot: std::sync::Mutex<Option<UnboundedReceiver<UiEvent>>>` + `pub fn take_rx(&self) -> Option<...>`（take 后返回 None）。
- 内核回合：`mcode_core::agent::runner::{run_agent_turn, AgentDeps, TurnOpts, MAX_TOOL_TURNS, ConfirmRequest}`
  - `run_agent_turn(deps: &Arc<AgentDeps>, user_input: &str, opts: TurnOpts)` -> String
  - TurnOpts 字段：instructions / max_turns / goal / tools_filter / attachments / history(&mut HistoryStore) / emit(EventSink)
  - AgentEvent（全部带语义）：TextDelta / Reasoning{text,scope} / Retrying{attempt,max,reason} /
    ToolStart{name,args,scope} / ToolResult{name,success,output,display:Option<DisplayBlock>,scope} / Usage(TokenUsage) / Compacted / Done{reply} / Error{message}
- 权限：`PermissionManager` 在 deps.perms (Arc<Mutex>)。App 自己也要有一个用于模式徽章显示 + cycle，
  直接用 deps 的（perms.lock()）+ 记住当前 Mode 缓存在 App.mode。
- 确认通道：runner 经 deps.confirm 发 ConfirmRequest{tool, args_summary, respond: oneshot::Sender<Decision>},
  runtime.rs 的确认泵已把它转成 UiEvent::Confirm 推进 ui 事件流。UI 收到后：
  set pending_permission；用户点击 允许/总是允许/拒绝 → respond.send(Decision)。
  注意 always_allow 沉淀：Decision::AllowAlways 时 perms.record 已在 runner 内做，UI 只需转发。
- 文件引用：`mcode_core::util::file_refs::{extract_refs, expand_file_refs}`，
  expand 返回 FileRefResult{text, injected, missing, attachments: Vec<(path, data_url)>}。
  多模态门控：`cfg.active_provider().map(|p| p.is_multimodal(model))`。
- 会话：`SessionStore::{create, list, load, append_message, ensure_title, add_checkpoint, meta}`。
- 文件树：App 自己用 walkdir 扫 cwd（深度3，跳 .git/target/node_modules，上限600）。
- 指令目录：`mcode_core::commands::{COMMANDS, filter_commands}`。
- 模式循环：`Mode::next()`（passon→manual→auto→plan）。

## runtime.rs 需要的改动

1. `rx_slot` 改造（见上）。
2. `Harness` 增加字段 `pub deps: Arc<AgentDeps>`（已有）。
3. build_harness 失败时（如无 provider / 无 key）：App 显示引导/设置提示，不 panic。

## App 结构（app.rs 重写）

```rust
pub struct App {
    focus_handle, input: Entity<TextInput>,
    rt: Arc<UiRuntime>,
    pub harness: Option<Arc<Harness>>,
    hist: Arc<Mutex<HistoryStore>>,      // run_agent_turn 消费
    pub messages: Vec<ChatMsg>,
    queue: Vec<String>,
    running: bool,
    completion + candidates + history(InputHistory),
    pub pending_permission: Option<PendingPermission { tool, args_summary, respond: Option<oneshot::Sender<Decision>> }>,
    pub sessions: Vec<SessionsMeta>,
    pub session: Option<RenderableSession /* SessionStore + 重建用 cwd */>,
    file_tree: Vec<String>,
    pub cwd: PathBuf,
    pub goal: Option<String>,
    pub tokens_total: u64,
}
```

## App 关键行为

- `App::new`：读 config → needs_onboarding ? View::Onboarding : build_harness → attach_pump。
  build_harness 失败（如 key 缺失）→ harness=None + messages 里放 Notice 提示去设置。
- `send(text)`：
  1. trim 空 → return；入 InputHistory。
  2. running → 入 queue，notify。
  3. '/' 开头 → handle_command（本地执行返回 true 就不进模型回合）。
  4. expand_file_refs(text, cwd, multimodal) → (expanded_text, attachments, missing/injected 提示 Notice)。
  5. session 为 None → SessionStore::create；ensure_title；append User 消息。
  send 里只推 UI 消息 User + Running 占位。
- `start_turn`：cancel_tx.send(false) → rt.spawn(async move {
    hist.lock() → run_agent_turn(&deps, &text, TurnOpts{ max_turns: MAX_TOOL_TURNS, goal, attachments, history, emit }) })
  emit = runtime::event_sink(h.ui_tx.clone())。
- `on_ui_event`: Agent(e) → on_agent_event；Confirm(req) → pending_permission = Some(...)。
- on_agent_event：
  - TextDelta：last==Running → 替换为 Assistant{t}；last==Assistant → append。
  - Reasoning：last==Thinking → append；否则 push Thinking。
  - ToolStart：push ToolCard{running:true}。ToolResult：找**最后一个**同名且 running 的 ToolCard 更新 status（✓/✗ + 摘要）；display Diff → push Diff 消息；display Output/FileRef 且内容非空 → push Notice（截断 400 字符）。
  - Usage：tokens_total += input+output；session.meta.tokens_total 同步。
  - Done{reply}：若 reply 非空且最后消息不是 Assistant(含 reply) → push Assistant；persist_assistant；finish_run（弹 Running、running=false、queue 非空则出队 → start_turn）。
  - Error{message}：push Notice 错误样式 + finish_run。
- 权限弹窗按钮：允许→Allow、总是允许→AllowAlways、拒绝→Deny；respond.take() 后 send。
  拒绝时 runner 已把 "denied by user" 作为工具结果回喂，UI 无需追加。
- 打断：Interrupt action / ESC（running 时）：cancel_tx.send(true)；事件会自然收尾（Done 空回复）。
- 新建会话：messages.clear、queue.clear、hist.clear、session=None、goal 保持。
- 指令执行 handle_command(text)->bool：
  /help /status /doctor /token /model /language /clear /compact /goal /agents /skills /memory /init /save /resume /pua:on /pua:off /config /permissions /hooks /mcp /exit + 未识别 → Notice "暂未支持"。
  - /clear：messages.clear + hist.clear + session=None
  - /compact [focus]：hist.lock().compact(...)，手动摘要=消息计数+首条 200 字符
  - /model [name]：有参 → 校验在 provider.models 中 → cfg.active_model=Some，cfg.save()，重建 harness（build_harness + attach_pump），Notice 确认；无参 → Notice 列出可选。
  /model 重建 harness 时注意：旧 harness drop 后确认泵退出，新 harness 需 attach_pump。
  - /language [code]：同理改 cfg.language save + rebuild。
  - /goal text|clear：App.goal = Some/clear；Notice。
  - /agents /skills /mcp /hooks：切到对应视图或 Notice 列表。
  - /save：把 messages 渲染成 markdown 快照写 ~/.mcode/session-data/<date>-<id>.md。
  - /resume：切 Sessions 视图。
  - /init：生成 MCODE.md 模板到 cwd（若不存在）。
  - /pua:on|off：改 cfg.pua_always_on + save + rebuild harness。
  - /exit：cx.quit()。

## 渲染（views.rs，impl App 的方法）

- `impl Render for App` 分发：Onboarding → render_onboarding；其余保持原型 chat 布局：
  顶栏（模型 badge=cfg.active_model / cwd / 模式徽章可点击 cycle + goal 徽章若有）+ 侧栏
  （新建会话、当前目录会话列表（真实 SessionStore::list，点击 → resume：SessionStore::load 恢复消息
  into messages（JSONL 富消息 → ChatMsg 映射：Text→User/Assistant、ToolUse→ToolCard、ToolResult→Notice），
  hist 重建（相邻 user/assistant 文本对）、tokens_total、切回 Chat）+ 上下文占用（hist.estimate_tokens() vs 160k）。
- Settings 视图：供应商信息 + 模型点击切换（重选 active_model → save + rebuild harness）、语言循环切换、
  default mode、pua 开关、权限规则列表（来源 desktop.json）、"重新运行引导"按钮 → Onboarding。
- Extensions 视图：MCP servers（desktop.json）：名称/命令/工具数（来自 harness.mcp_report）+ 失败原因；
  hooks（desktop.json）：event/matcher/command 列表；提示"编辑 ~/.mcode/desktop.json 后重启生效"。
- Sessions 视图：会话列表（title/turns/updated），点击 → resume 恢复消息流回 Chat。
- Onboarding 视图：表单（4 个 TextInput：名称 / baseURL / apiKeyEnv / models 逗号分隔）+
  保存 → Config 构造 + save() → build_harness → attach_pump → View::Chat。
- 补全面板/排队徽章/权限弹窗/Running 指示照原型渲染。
- 注意 gpui 0.2：div() 链式、.when()、.children()、.map()、cx.listener、.on_click 需要 .id() 的元素；
  overflow_y_scroll 需 .id()（Stateful）。
