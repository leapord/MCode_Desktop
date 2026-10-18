//! 主应用实体 — 全接线（M2）：真实流式回合（run_agent_turn）、权限弹窗真裁决（Confirm 通道）、
//! /goal 外层循环（check_goal_complete 裁判）、会话持久化（SessionStore）、指令系统。
//! 渲染分发在 views.rs（四视图）与 chat_view.rs（chat 布局），指令与会话操作在 session_ops.rs。

use gpui::{
    actions, prelude::*, App as GpuiApp, Context, Entity, FocusHandle, Focusable, Render,
    SharedString, Window,
};
use mcode_core::agent::completion::{Candidate, CompletionState, InputHistory, Trigger};
use mcode_core::agent::context::{build_goal_prompt, MAX_GOAL_ROUNDS};
use mcode_core::config::Config;
use mcode_core::agent::history::HistoryStore;
use mcode_core::agent::runner::{run_agent_turn, TurnOpts, MAX_TOOL_TURNS};
use mcode_core::agent::{AgentEvent, Attachment, Message, Role};
use mcode_core::permission::{Decision, Mode};
use mcode_core::sessions::{SessionMeta, SessionStore};
use mcode_core::tools::DisplayBlock;
use mcode_core::util::file_refs::{expand_file_refs, extract_refs};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::runtime::{event_sink, Harness, UiEvent, UiRuntime};
use crate::text_input::{InputEvent, TextInput};

actions!(
    app,
    [
        CycleMode,
        PaletteUp,
        PaletteDown,
        PaletteComplete,
        ClosePalette,
        NewChat,
        OpenSettings,
        OpenExtensions,
        OpenSessions,
        BackToChat
    ]
);

// ---------- 视图与消息视图模型 ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Chat,
    Settings,
    Extensions,
    Sessions,
    Onboarding,
}

/// 消息流视图模型（对齐 prototype chat.html 的消息形态）
#[derive(Clone)]
pub enum ChatMsg {
    User {
        text: SharedString,
        refs: Vec<SharedString>,
    },
    Assistant {
        text: SharedString,
    },
    /// 思考过程（连续 Reasoning 增量合并）
    Thinking {
        text: SharedString,
    },
    /// 工具调用卡片（MCP 工具名为运行时字符串，name 不用 &'static str）
    ToolCard {
        name: SharedString,
        args: SharedString,
        summary: SharedString,
        running: bool,
        success: bool,
    },
    Diff {
        path: SharedString,
        lines: Vec<DiffLine>,
    },
    Notice {
        text: SharedString,
        error: bool,
    },
    /// 回合运行占位（finish_run 时移除）
    Running,
}

#[derive(Clone)]
pub struct DiffLine {
    pub add: bool,
    pub del: bool,
    pub hunk: bool,
    pub text: SharedString,
}

/// 权限确认弹窗的待裁决请求（respond oneshot 回传 runner）
pub struct PendingPermission {
    pub tool: String,
    pub args_summary: String,
    pub respond: Option<tokio::sync::oneshot::Sender<Decision>>,
}

/// App — UI 侧状态 + 内核接线。回合一启动就持 hist 锁推进，UI 线程只 try_lock 只读。
pub struct App {
    focus_handle: FocusHandle,
    /// Composer 输入框
    pub input: Entity<TextInput>,
    /// Onboarding 表单输入（名称 / baseURL / apiKeyEnv / models 逗号分隔）
    pub ob_name: Entity<TextInput>,
    pub ob_url: Entity<TextInput>,
    pub ob_key: Entity<TextInput>,
    pub ob_models: Entity<TextInput>,
    rt: Arc<UiRuntime>,
    pub harness: Option<Arc<Harness>>,
    pub cfg: Config,
    /// run_agent_turn 消费的历史（回合任务持锁期间 UI 用 try_lock 只读）
    pub(crate) hist: Arc<Mutex<HistoryStore>>,
    pub messages: Vec<ChatMsg>,
    pub(crate) queue: Vec<String>,
    pub(crate) running: bool,
    pub(crate) completion: CompletionState,
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) history: InputHistory,
    /// 当前权限模式（缓存；真值在 harness.deps.perms，UI try_lock 读写）
    pub mode: Mode,
    pub pending_permission: Option<PendingPermission>,
    pub sessions: Vec<SessionMeta>,
    pub session: Option<SessionStore>,
    pub file_tree: Vec<String>,
    pub cwd: PathBuf,
    pub goal: Option<String>,
    /// goal 外层循环已跑的轮数（/goal clear 时清零）
    pub goal_rounds: usize,
    pub tokens_total: u64,
    /// 上次估算的上下文 token（回合运行中 hist 锁被回合任务持有，渲染用缓存）
    pub(crate) ctx_tokens: usize,
    pub view: View,
    // -- 工作区文件浏览器 / 布局 --
    /// 左侧功能区折叠（40px 图标条）
    pub(crate) sidebar_collapsed: bool,
    /// 右侧文件浏览器可见
    pub(crate) explorer_visible: bool,
    /// 文件树展开的目录（默认折叠到深度 2：cwd 本层与次层目录预展开）
    pub(crate) expanded_dirs: std::collections::HashSet<PathBuf>,
    /// 浏览器内选中项（复制/剪切/删除/引用的目标）
    pub(crate) fs_selected: Option<std::path::PathBuf>,
    /// 文件剪贴板 (path, is_cut)
    pub(crate) fs_clipboard: Option<(std::path::PathBuf, bool)>,
    /// 删除二次确认（第一次点击武装，同一路径再点一次执行）
    pub(crate) fs_delete_armed: Option<std::path::PathBuf>,
    /// 文件浏览器键上下文的焦点句柄（cmd-c/x/v 拦截）
    pub explorer_focus: gpui::FocusHandle,
}

/// 截断显示文本
pub(crate) fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

impl App {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let cfg = Config::load();
        let cwd = std::env::current_dir().unwrap_or_default();
        let input = Self::make_input(cx, "向 MCode 发送消息…  （/ 指令 · @ 文件）");
        let ob_name = Self::make_input(cx, "如 zhipu / glm / openai");
        let ob_url = Self::make_input(cx, "https://open.bigmodel.cn/api/paas/v4");
        let ob_key = Self::make_input(cx, "API key 环境变量名，如 ZHIPU_API_KEY");
        let ob_models = Self::make_input(cx, "逗号分隔，如 glm-4.7-multimodal,glm-4.7-air");
        let needs_onboarding = cfg.needs_onboarding();
        let mut me = Self {
            focus_handle: cx.focus_handle(),
            input,
            ob_name,
            ob_url,
            ob_key,
            ob_models,
            rt: Arc::new(UiRuntime::new().expect("tokio runtime 创建失败")),
            harness: None,
            cfg: cfg.clone(),
            hist: Arc::new(Mutex::new(HistoryStore::default())),
            messages: vec![],
            queue: vec![],
            running: false,
            completion: CompletionState::default(),
            candidates: vec![],
            history: InputHistory::default(),
            mode: cfg.default_mode,
            pending_permission: None,
            sessions: vec![],
            session: None,
            file_tree: scan_file_tree(&cwd),
            cwd,
            goal: None,
            goal_rounds: 0,
            tokens_total: 0,
            ctx_tokens: 0,
            view: if needs_onboarding {
                View::Onboarding
            } else {
                View::Chat
            },
            sidebar_collapsed: false,
            explorer_visible: true,
            expanded_dirs: Default::default(),
            fs_selected: None,
            fs_clipboard: None,
            fs_delete_armed: None,
            explorer_focus: cx.focus_handle(),
        };
        // 文件树默认展开到深度 2（cwd 本层与次层目录预展开）
        me.prefill_expanded();
        // 输入框事件 → 发送 / 补全状态机同步（对齐 prototype 的 useCompletion 链路）
        cx.subscribe(&me.input, |this, _, event, cx| match event {
            InputEvent::Submit(text) => this.send(text.clone(), cx),
            InputEvent::Changed(text) => this.sync_completion(text),
        })
        .detach();
        // 非 onboarding 启动：构建 harness（失败 → Notice 引导去设置，不 panic）
        if !needs_onboarding {
            me.rebuild_harness(cx);
        }
        me
    }

    fn make_input(cx: &mut Context<Self>, placeholder: &str) -> Entity<TextInput> {
        cx.new(|cx| TextInput {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.to_string().into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        })
    }

    // ---------- harness 生命周期 ----------

    /// 构建 harness 并挂事件泵；失败不 panic（Notice 引导去设置）
    pub fn rebuild_harness(&mut self, cx: &mut Context<Self>) {
        self.harness = match crate::runtime::build_harness(&self.cfg, &self.cwd, &self.rt) {
            Ok(h) => Some(Arc::new(h)),
            Err(e) => {
                self.messages.push(ChatMsg::Notice {
                    text: format!("初始化失败：{e}（cmd-, 打开设置）").into(),
                    error: true,
                });
                None
            }
        };
        if self.harness.is_some() {
            self.attach_pump(cx);
        }
        self.refresh_sessions();
    }

    /// 挂接 GPUI 事件泵：tokio channel → 主线程实体更新（WeakEntity::update，实体释放即退出）
    fn attach_pump(&mut self, cx: &mut Context<Self>) {
        let Some(h) = self.harness.clone() else { return };
        let Some(mut rx) = h.take_rx() else { return };
        cx.spawn(async move |this, cx| {
            while let Some(ev) = rx.recv().await {
                if this.update(cx, |app, cx| app.on_ui_event(ev, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    // ---------- 发送 / 回合启动 ----------

    /// Composer 提交入口
    fn send(&mut self, text: String, cx: &mut Context<Self>) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.completion.close();
        self.history.push(text.clone());
        // running 时入队（指令出队时再解释）
        if self.running {
            self.queue.push(text);
            cx.notify();
            return;
        }
        // 指令本地解释，不进模型回合
        if text.starts_with('/') && self.handle_command(&text, cx) {
            cx.notify();
            return;
        }
        self.launch_turn(text, cx);
    }

    /// 启动一次模型回合：@ 引用展开 → 会话落盘 → UI 占位 → spawn run_agent_turn
    fn launch_turn(&mut self, text: String, cx: &mut Context<Self>) {
        let mut turn_input = text.clone();
        let mut attachments = vec![];

        // -- @ 文件引用展开（multimodal 门控在内核 expand_file_refs 内） --
        if text.contains('@') {
            let multimodal = self
                .cfg
                .active_provider()
                .zip(self.cfg.active_model())
                .map(|(p, m)| p.is_multimodal(&m))
                .unwrap_or(false);
            let r = expand_file_refs(&text, &self.cwd, multimodal);
            if !r.injected.is_empty() {
                self.push_notice(format!("📎 已注入 {} 个文件引用", r.injected.len()), false);
            }
            if !r.missing.is_empty() {
                self.push_notice(format!("⚠ 引用未解析：{}", r.missing.join("、")), true);
            }
            for (p, url) in r.attachments {
                attachments.push(Attachment { path: p, data_url: url });
            }
            turn_input = r.text;
        }

        // -- 会话持久化：无会话则创建 + 标题 + User 消息落盘 --
        if self.session.is_none() {
            self.session = SessionStore::create(&self.cwd).ok();
        }
        if let Some(s) = self.session.as_mut() {
            s.ensure_title(&text);
            s.meta.model = self.cfg.active_model().unwrap_or_default();
            s.meta.mode = self.mode.label().to_string();
            let m = Message::text(Role::User, turn_input.clone());
            let _ = s.append_message(&m);
        }

        // -- UI 消息：User（带 @ 引用 chips）+ Running 占位 --
        let refs: Vec<SharedString> = extract_refs(&text)
            .into_iter()
            .map(SharedString::from)
            .collect();
        self.messages.push(ChatMsg::User { text: text.into(), refs });
        self.messages.push(ChatMsg::Running);
        self.running = true;
        self.start_turn(turn_input, attachments, cx);
        cx.notify();
    }

    /// spawn run_agent_turn（rt 线程：持 hist 锁推进，事件经 ui_tx → 泵回主线程）
    fn start_turn(&mut self, input: String, attachments: Vec<Attachment>, cx: &mut Context<Self>) {
        let Some(h) = self.harness.clone() else {
            self.running = false;
            self.pop_running();
            self.push_notice("未初始化完成，无法启动回合（cmd-, 打开设置）".to_string(), true);
            cx.notify();
            return;
        };
        let _ = h.deps.cancel_tx.send(false);
        let deps = h.deps.clone();
        let hist = self.hist.clone();
        let sink = event_sink(h.ui_tx.clone());
        let goal = self.goal.clone();
        self.rt.spawn(async move {
            let mut hs = hist.lock().await;
            run_agent_turn(
                &deps,
                &input,
                TurnOpts {
                    instructions: None,
                    max_turns: MAX_TOOL_TURNS,
                    goal,
                    tools_filter: None,
                    attachments,
                    history: &mut hs,
                    emit: sink,
                },
            )
            .await;
        });
        cx.notify();
    }

    // ---------- goal 外层循环（对齐 CLI cli.tsx runTurn） ----------

    /// spawn goal 完成裁判（独立单发调用，结果经 GoalCheck 事件回主线程）
    fn spawn_goal_check(&mut self) {
        let Some(h) = self.harness.clone() else { return };
        let Some(goal) = self.goal.clone() else { return };
        let backend = h.deps.backend.clone();
        let cancel = h.deps.cancel_tx.subscribe();
        let ui_tx = h.ui_tx.clone();
        self.rt.spawn(async move {
            let done = mcode_core::agent::context::check_goal_complete(backend.as_ref(), &goal, cancel).await;
            let _ = ui_tx.send(UiEvent::GoalCheck(done));
        });
    }

    /// goal 裁判结果：达成 → 收官 + drain 队列；未达成 → 以 goal prompt 再开一轮。
    /// 回合已被打断/出错收尾时，裁判结果是陈旧事件，直接丢弃。
    fn on_goal_check(&mut self, done: bool, cx: &mut Context<Self>) {
        if !self.running || self.goal.is_none() {
            return;
        }
        let goal = self.goal.clone().unwrap_or_default();
        if done {
            self.push_notice("🎯 goal 已达成".to_string(), false);
            self.goal = None;
            self.goal_rounds = 0;
            self.finish_run(cx);
        } else if self.goal_rounds >= MAX_GOAL_ROUNDS {
            self.push_notice(
                format!("🎯 goal 已达 {MAX_GOAL_ROUNDS} 轮上限，循环终止"),
                true,
            );
            self.goal = None;
            self.goal_rounds = 0;
            self.finish_run(cx);
        } else {
            self.goal_rounds += 1;
            self.push_notice(format!("🎯 goal 第 {} 轮", self.goal_rounds), false);
            let prompt = build_goal_prompt(&goal);
            self.start_turn(prompt, vec![], cx);
        }
    }

    // ---------- 内核事件处理 ----------

    /// 事件泵入口：Agent 事件 / 权限确认 / goal 裁判
    fn on_ui_event(&mut self, ev: UiEvent, cx: &mut Context<Self>) {
        match ev {
            UiEvent::Agent(e) => self.on_agent_event(e, cx),
            UiEvent::Confirm(req) => {
                self.pending_permission = Some(PendingPermission {
                    tool: req.tool,
                    args_summary: req.args_summary,
                    respond: Some(req.respond),
                });
            }
            UiEvent::GoalCheck(done) => self.on_goal_check(done, cx),
        }
        cx.notify();
    }

    fn on_agent_event(&mut self, ev: AgentEvent, cx: &mut Context<Self>) {
        match ev {
            // 文本增量：Running 占位 → Assistant 起步；已有 Assistant 则续写
            AgentEvent::TextDelta(t) => {
                if let Some(last) = self.messages.last_mut() {
                    match last {
                        ChatMsg::Running => *last = ChatMsg::Assistant { text: t.into() },
                        ChatMsg::Assistant { text } => *text = format!("{text}{t}").into(),
                        _ => self.messages.push(ChatMsg::Assistant { text: t.into() }),
                    }
                } else {
                    self.messages.push(ChatMsg::Assistant { text: t.into() });
                }
            }
            // 思考增量：连续 Reasoning 合并进最后一个 Thinking 块
            AgentEvent::Reasoning { text, .. } => {
                if let Some(ChatMsg::Thinking { text: t }) = self.messages.last_mut() {
                    *t = format!("{t}{text}").into();
                } else {
                    self.messages.push(ChatMsg::Thinking { text: text.into() });
                }
            }
            AgentEvent::Retrying { attempt, max, reason } => {
                self.push_notice(format!("↻ 流式重试 {attempt}/{max}：{reason}"), false);
            }
            // 工具卡起步：push 卡片（结果事件按"最后一个同名 running 卡"回填）
            AgentEvent::ToolStart { name, args, .. } => {
                self.messages.push(ChatMsg::ToolCard {
                    name: name.into(),
                    args: trunc(&args.to_string(), 200).into(),
                    summary: "".into(),
                    running: true,
                    success: false,
                });
            }
            // 工具卡收尾：回填状态 + display 块展开为 Diff/Notice 消息
            AgentEvent::ToolResult {
                name,
                success,
                output,
                display,
                ..
            } => {
                let summary = trunc(&output, 60);
                if let Some(card) = self
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|m| matches!(m, ChatMsg::ToolCard { name: n, running: true, .. } if n.as_ref() == name.as_str()))
                {
                    if let ChatMsg::ToolCard { summary: s, running, success: ok, .. } = card {
                        *s = summary.into();
                        *running = false;
                        *ok = success;
                    }
                }
                match display {
                    Some(DisplayBlock::Diff { path, lines }) => {
                        let mapped = lines
                            .iter()
                            .map(|l| DiffLine {
                                add: l.kind == mcode_core::tools::DiffKind::Add,
                                del: l.kind == mcode_core::tools::DiffKind::Delete,
                                hunk: l.kind == mcode_core::tools::DiffKind::Hunk,
                                text: l.text.clone().into(),
                            })
                            .collect();
                        self.messages.push(ChatMsg::Diff { path: path.into(), lines: mapped });
                    }
                    Some(DisplayBlock::Output { stdout, stderr, code }) => {
                        let mut text = stdout;
                        if !stderr.is_empty() {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(&stderr);
                        }
                        if !text.trim().is_empty() {
                            self.push_notice(
                                format!("▸ {name} · exit {code}\n{}", trunc(text.trim(), 400)),
                                false,
                            );
                        }
                    }
                    Some(DisplayBlock::FileRef { path, lines }) => {
                        if !lines.trim().is_empty() {
                            self.push_notice(format!("📄 {path}\n{}", trunc(lines.trim(), 400)), false);
                        }
                    }
                    None => {}
                }
            }
            // 用量累计：tokens_total + 会话 meta 同步
            AgentEvent::Usage(u) => {
                self.tokens_total += u.input + u.output;
                if let Some(s) = self.session.as_mut() {
                    s.meta.tokens_total = self.tokens_total;
                }
            }
            AgentEvent::Compacted { note } => {
                self.push_notice(format!("🗜 已压缩历史：{note}"), false);
            }
            // 回合收尾：goal 激活时优先推进 goal 循环（queue 等 goal 结束再消化）
            AgentEvent::Done { reply } => {
                if !reply.is_empty() && !matches!(self.messages.last(), Some(ChatMsg::Assistant { .. })) {
                    self.messages.push(ChatMsg::Assistant { text: reply.clone().into() });
                }
                if !reply.is_empty() {
                    if let Some(s) = self.session.as_mut() {
                        let m = Message::text(Role::Assistant, reply.clone());
                        let _ = s.append_message(&m);
                        s.meta.turns += 1;
                    }
                }
                if self.goal.is_some() && !reply.is_empty() {
                    self.spawn_goal_check();
                } else {
                    if self.goal.is_some() {
                        self.push_notice("🎯 goal 循环已终止（回合被打断或出错）".to_string(), false);
                        self.goal = None;
                        self.goal_rounds = 0;
                    }
                    self.finish_run(cx);
                }
                self.refresh_ctx();
            }
            AgentEvent::Error { message } => {
                self.push_notice(format!("✗ {message}"), true);
                self.finish_run(cx);
            }
        }
    }

    // ---------- 回合收尾 / 队列 / 打断 / 权限 ----------

    /// 回合收尾：移除 Running 占位，队列非空则出队启动下一轮
    fn finish_run(&mut self, cx: &mut Context<Self>) {
        self.pop_running();
        self.running = false;
        if let Some(next) = self.queue.first().cloned() {
            self.queue.remove(0);
            self.launch_turn(next, cx);
        }
    }

    fn pop_running(&mut self) {
        if matches!(self.messages.last(), Some(ChatMsg::Running)) {
            self.messages.pop();
        }
    }

    /// ESC：关补全 → 拒绝待裁决权限 → 打断回合 → 返回 Chat 视图
    pub(crate) fn interrupt(&mut self, _cx: &mut Context<Self>) {
        if self.completion.is_open() {
            self.completion.close();
            return;
        }
        if let Some(p) = self.pending_permission.as_mut() {
            if let Some(tx) = p.respond.take() {
                let _ = tx.send(Decision::Deny);
            }
            self.pending_permission = None;
            return;
        }
        if self.running {
            if let Some(h) = &self.harness {
                let _ = h.deps.cancel_tx.send(true);
            }
            return;
        }
        if self.view != View::Chat {
            self.view = View::Chat;
        }
    }

    /// 权限弹窗裁决：respond oneshot 回传 runner（AllowAlways 的沉淀在 runner 内）
    pub(crate) fn resolve_permission(&mut self, decision: Decision, cx: &mut Context<Self>) {
        if let Some(p) = self.pending_permission.take() {
            if let Some(tx) = p.respond {
                let _ = tx.send(decision);
            }
        }
        cx.notify();
    }

    // ---------- 补全状态机 ----------

    fn sync_completion(&mut self, text: &str) {
        self.completion.sync_from_input(text);
        if self.completion.is_open() {
            let q = self.completion.query.clone();
            self.candidates = match self.completion.trigger {
                Some(Trigger::Slash) => mcode_core::commands::filter_commands(&q)
                    .into_iter()
                    .map(|c| Candidate { name: c.name.into(), desc: c.desc.into(), is_dir: false })
                    .collect(),
                Some(Trigger::At) => self
                    .file_tree
                    .iter()
                    .filter(|f| f.to_lowercase().contains(&q.to_lowercase()))
                    .take(12)
                    .map(|f| Candidate {
                        name: f.clone(),
                        desc: String::new(),
                        is_dir: f.ends_with('/'),
                    })
                    .collect(),
                None => vec![],
            };
        } else {
            self.candidates.clear();
        }
    }

}

impl Focusable for App {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::chat_view::render_app(self, window, cx)
    }
}

// ---------- 文件树扫描（App 自己扫 cwd：深度 3，跳 .git/target/node_modules，上限 600） ----------

pub(crate) fn scan_file_tree(cwd: &PathBuf) -> Vec<String> {
    let mut out = vec![];
    walk_dir(cwd, cwd, 0, &mut out);
    out
}

fn walk_dir(base: &std::path::Path, dir: &std::path::Path, depth: usize, out: &mut Vec<String>) {
    if depth >= 3 || out.len() >= 600 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if out.len() >= 600 {
            return;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), ".git" | "target" | "node_modules" | ".mcode") {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            out.push(format!("{}/", e.path().strip_prefix(base).unwrap_or(&p).to_string_lossy()));
            walk_dir(base, &p, depth + 1, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_string_lossy().into_owned());
        }
    }
}
