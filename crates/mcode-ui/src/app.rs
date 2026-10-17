//! 主界面 — 布局与交互对齐 desktop-prototype/screens/chat.html：
//! 侧栏（会话/占用）+ 顶栏（模型/路径/模式徽章）+ 消息流（工具卡/diff）+ Composer（/ 与 @ 补全、排队、打断）

use gpui::{
    actions, div, prelude::*, px, rgb, App as GpuiApp, Context, Entity, FocusHandle, Focusable,
    Render, Rgba, SharedString, Window,
};
use mcode_core::agent::completion::{Candidate, CompletionState, InputHistory, Trigger};
use mcode_core::commands as cmd;
use mcode_core::permission::{Mode, PermissionManager, Verdict};

use crate::text_input::{InputEvent, TextInput};
use crate::theme::*;

actions!(app, [CycleMode, PaletteUp, PaletteDown, PaletteComplete, ClosePalette, NewChat, OpenSettings, Interrupt]);

// ---------- 消息视图模型 ----------

#[derive(Clone)]
pub enum ChatMsg {
    User { text: SharedString, refs: Vec<SharedString> },
    Assistant { text: SharedString },
    ToolCard { kind: &'static str, name: &'static str, args: SharedString, status: SharedString },
    Diff { path: SharedString, lines: Vec<(bool, bool, SharedString)> }, // (add, del, text)
    Thinking,
    Running,
}

pub struct App {
    focus_handle: FocusHandle,
    input: Entity<TextInput>,
    pub mode: Mode,
    perms: PermissionManager,
    messages: Vec<ChatMsg>,
    queue: Vec<String>,
    running: bool,
    completion: CompletionState,
    candidates: Vec<Candidate>,
    history: InputHistory,
    pending_permission: Option<(String, String)>,
    sessions: Vec<(SharedString, SharedString)>,
}

const FILE_TREE: &[&str] = &[
    "src/", "src/agent/", "src/agent/loop.ts", "src/agent/harness/", "src/agent/harness/compaction.ts",
    "src/api/client.ts", "src/commands/builtin.ts", "src/config/config.ts", "src/tools/bash.ts",
    "src/tools/edit-file.ts", "MCode.md", "package.json",
];

impl App {
    pub fn new(cx: &mut Context<Self>, input: Entity<TextInput>) -> Self {
        // 种子消息：对齐原型 chat.html 的静态会话内容
        let messages = vec![
            ChatMsg::User {
                text: "看下 client.ts 里图片附件是怎么传给模型的，识别一直失败，帮我对照排查。".into(),
                refs: vec!["@src/api/client.ts".into(), "@画布/整体规划.png".into()],
            },
            ChatMsg::Thinking,
            ChatMsg::ToolCard { kind: "read", name: "read_file", args: "src/api/client.ts · L1-120".into(), status: "✓ 120 行".into() },
            ChatMsg::ToolCard { kind: "bash", name: "bash", args: "grep -n \"multimodal\" src/config/config.ts".into(), status: "✓ exit 0".into() },
            ChatMsg::Diff {
                path: "src/api/client.ts".into(),
                lines: vec![
                    (false, false, "  const parts: Part[] = [{ type: 'text', text: msg.text }]".into()),
                    (false, true, "-    // BUG: 未检查 model.multimodal，直接拼接 image_url".into()),
                    (true, false, "+    if (!activeModel.multimodal) {".into()),
                    (true, false, "+      // 非多模态模型：拒绝图片，回传提示文本".into()),
                    (true, false, "+      continue".into()),
                    (false, false, "  }".into()),
                ],
            },
            ChatMsg::ToolCard { kind: "task", name: "task", args: "code-explorer → 独立上下文 · 只读白名单".into(), status: "◌ 运行中".into() },
            ChatMsg::Assistant { text: "已定位并修复：client.ts:46 拼接图片前没有校验 model.multimodal。非多模态模型的图片附件会被拒绝并降级为文本提示。".into() },
        ];
        let cfg = mcode_core::config::Config::load();
        let mut sessions = vec![];
        if let Ok(list) = mcode_core::sessions::SessionStore::list(&std::env::current_dir().unwrap_or_default()) {
            for m in list.iter().take(8) {
                sessions.push((m.title.clone().into(), format!("{}轮", m.turns).into()));
            }
        }
        if sessions.is_empty() {
            sessions = vec![
                ("修复多模态图片识别异常".into(), "12轮".into()),
                ("three.min.js 加载排查".into(), "08轮".into()),
                ("/pua:on 全局开启配置".into(), "03轮".into()),
            ];
        }
        let me = Self {
            focus_handle: cx.focus_handle(),
            input: input.clone(),
            perms: PermissionManager::new(cfg.default_mode, cfg.permission_rules.clone()),
            mode: cfg.default_mode,
            messages,
            queue: vec![],
            running: false,
            completion: CompletionState::default(),
            candidates: vec![],
            history: InputHistory::default(),
            pending_permission: None,
            sessions,
        };
        // 输入框事件 → 发送 / 补全状态机同步（对齐 prototype 的 useCompletion 链路）
        cx.subscribe(&input, |this, _, event, cx| match event {
            InputEvent::Submit(text) => this.send(text.clone(), cx),
            InputEvent::Changed(text) => this.sync_completion(text),
        })
        .detach();
        me
    }

    // ---------- 交互 ----------

    fn sync_completion(&mut self, text: &str) {
        self.completion.sync_from_input(text);
        if self.completion.is_open() {
            let q = self.completion.query.clone();
            self.candidates = match self.completion.trigger {
                Some(Trigger::Slash) => cmd::filter_commands(&q)
                    .into_iter()
                    .map(|c| Candidate { name: c.name.into(), desc: c.desc.into(), is_dir: false })
                    .collect(),
                Some(Trigger::At) => FILE_TREE
                    .iter()
                    .filter(|f| f.to_lowercase().contains(&q.to_lowercase()))
                    .take(12)
                    .map(|f| Candidate {
                        name: f.to_string(),
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

    fn send(&mut self, text: String, cx: &mut Context<Self>) {
        self.completion.close();
        self.history.push(text.clone());
        if self.running {
            self.queue.push(text);
            cx.notify();
            return;
        }
        self.messages.push(ChatMsg::User { text: text.clone().into(), refs: vec![] });
        self.messages.push(ChatMsg::Running);
        self.running = true;

        // 权限门控演示：manual 模式下非只读工具需要确认（真实门控在内核 PermissionManager::evaluate）
        if self.mode == Mode::Manual && matches!(self.perms.evaluate("bash", false, "*"), Verdict::Confirm) {
            self.pending_permission = Some(("bash".into(), "npm test\n\n（manual 模式：非只读工具执行前必须确认）".into()));
        }
        // TODO(M2): 接通 mcode-core ResponsesBackend 真实流式回合；
        // 当前初始化里程碑以静态回复占位验证全链路 UI。
        self.messages.push(ChatMsg::Assistant {
            text: "（M2 接通内核）本轮消息已进入 harness 历史与 SessionStore 持久化链路。".into(),
        });
        cx.notify();
    }

    fn finish_run(&mut self) {
        // 移除 Running 占位
        if matches!(self.messages.last(), Some(ChatMsg::Running)) {
            self.messages.pop();
        }
        self.running = false;
        if let Some(next) = self.queue.first().cloned() {
            self.queue.remove(0);
            self.messages.push(ChatMsg::User { text: next.into(), refs: vec![] });
            self.messages.push(ChatMsg::Running);
            self.running = true;
        }
    }

    fn interrupt(&mut self, cx: &mut Context<Self>) {
        if self.running {
            self.finish_run();
            cx.notify();
        }
    }

    // ---------- action handlers ----------

    fn on_cycle_mode(&mut self, _: &CycleMode, _: &mut Window, cx: &mut Context<Self>) {
        self.mode = self.perms.cycle_mode();
        cx.notify();
    }

    fn on_palette_up(&mut self, _: &PaletteUp, _: &mut Window, cx: &mut Context<Self>) {
        self.completion.move_up(self.candidates.len());
        cx.notify();
    }

    fn on_palette_down(&mut self, _: &PaletteDown, _: &mut Window, cx: &mut Context<Self>) {
        self.completion.move_down(self.candidates.len());
        cx.notify();
    }

    fn on_palette_complete(&mut self, _: &PaletteComplete, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.candidates.get(self.completion.selected).cloned() {
            let text = self.input.read(cx).text();
            let new_text = self.completion.apply(&text, &c.name);
            self.input.update(cx, |inp, cx| inp.set_text(&new_text, cx));
        }
        self.completion.close();
        cx.notify();
    }

    fn on_close_palette(&mut self, _: &ClosePalette, _: &mut Window, cx: &mut Context<Self>) {
        self.completion.close();
        cx.notify();
    }

    fn on_new_chat(&mut self, _: &NewChat, _: &mut Window, cx: &mut Context<Self>) {
        self.messages.clear();
        self.queue.clear();
        self.running = false;
        cx.notify();
    }

    fn on_open_settings(&mut self, _: &OpenSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.messages.push(ChatMsg::Assistant { text: "设置中心视图（views/settings）— M3 接入".into() });
        cx.notify();
    }

    fn on_permission(&mut self, allow: bool, cx: &mut Context<Self>) {
        if let Some((tool, _)) = self.pending_permission.take() {
            if allow {
                self.perms.record(&tool, &mcode_core::Decision::Allow);
            }
        }
        cx.notify();
    }
}

impl Focusable for App {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// ---------- 渲染 ----------

fn badge(label: impl Into<SharedString>, color: Rgba) -> gpui::Div {
    div()
        .px_2()
        .py_0p5()
        .rounded_full()
        .text_size(px(11.))
        .text_color(color)
        .border_1()
        .border_color(color)
        .child(label.into())
}

fn tool_icon(kind: &str) -> (&'static str, Rgba) {
    match kind {
        "read" => ("📄", BLUE),
        "bash" => ("▶", YELLOW),
        "task" => ("⚡", PURPLE),
        _ => ("✏️", GREEN),
    }
}

impl Render for App {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mc = mode_color(self.mode);
        let mode_label: SharedString = format!("⇥ {}", self.mode.label()).into();

        // 消息流
        let message_list = self.messages.iter().map(|m| match m {
            ChatMsg::User { text, refs } => div()
                .mb_3()
                .child(
                    div()
                        .rounded_lg()
                        .border_1()
                        .border_color(BORDER)
                        .bg(BG_PANEL)
                        .px_3()
                        .py_2()
                        .text_color(TEXT_1)
                        .child(text.clone()),
                )
                .children(refs.iter().map(|r| {
                    div()
                        .mr_1()
                        .mt_1()
                        .px_1p5()
                        .rounded_sm()
                        .text_size(px(11.))
                        .text_color(BLUE)
                        .child(r.clone())
                })),
            ChatMsg::Assistant { text } => div()
                .mb_3()
                .text_color(TEXT_1)
                .child(text.clone()),
            ChatMsg::Thinking => div()
                .mb_3()
                .text_color(TEXT_3)
                .text_size(px(12.))
                .child("💭 思考过程 · 已折叠"),
            ChatMsg::Running => div().mb_3().text_color(ACCENT).child("◌ 运行中…  （ESC 打断）"),
            ChatMsg::ToolCard { kind, name, args, status } => {
                let (icon, color) = tool_icon(kind);
                div()
                    .mb_2()
                    .rounded_md()
                    .border_1()
                    .border_color(BORDER)
                    .bg(BG_PANEL)
                    .px_3()
                    .py_1p5()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .child(div().text_color(color).child(icon.to_string()))
                    .child(div().text_color(TEXT_1).child(name.to_string()))
                    .child(div().text_color(TEXT_3).child(args.clone()))
                    .child(div().ml_auto().text_color(TEXT_3).child(status.clone()))
            }
            ChatMsg::Diff { path, lines } => {
                let mut wrap = div()
                    .mb_2()
                    .rounded_md()
                    .border_1()
                    .border_color(BORDER)
                    .bg(BG_WINDOW)
                    .overflow_hidden()
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .text_size(px(11.))
                            .text_color(TEXT_3)
                            .child(path.clone()),
                    );
                for (add, del, text) in lines {
                    let (bg, fg) = if *add {
                        (rgb(0x1b2a1e), rgb(0x9fd9a3))
                    } else if *del {
                        (rgb(0x2d1b1b), rgb(0xeb9a9a))
                    } else {
                        (BG_WINDOW, TEXT_3)
                    };
                    wrap = wrap.child(
                        div().bg(bg).px_3().text_size(px(11.5)).font_family("Menlo").text_color(fg).child(text.clone()),
                    );
                }
                wrap
            }
        }).collect::<Vec<_>>();

        // 补全面板
        let palette_open = self.completion.is_open() && !self.candidates.is_empty();
        let palette_items: Vec<_> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let sel = i == self.completion.selected;
                div()
                    .px_3()
                    .py_1()
                    .text_size(px(12.))
                    .flex()
                    .gap_2()
                    .when(sel, |d| d.bg(rgb(0x3a2a24)))
                    .child(div().text_color(if sel { ACCENT } else { TEXT_1 }).font_family("Menlo").child(c.name.clone()))
                    .when(!c.desc.is_empty(), |d| d.child(div().text_color(TEXT_3).child(c.desc.clone())))
            })
            .collect();

        // 排队插话
        let queue_banner = if !self.queue.is_empty() {
            div().px_6().pb_1().child(
                div()
                    .rounded_full()
                    .border_1()
                    .border_color(BLUE)
                    .text_color(BLUE)
                    .px_3()
                    .py_1()
                    .text_size(px(11.5))
                    .child(format!("📥 {} 条排队插话", self.queue.len())),
            )
        } else {
            div()
        };

        // 权限弹窗
        let perm_overlay = self.pending_permission.as_ref().map(|(tool, args)| {
            let tool: SharedString = tool.clone().into();
            let args: SharedString = args.clone().into();
            div()
                .absolute()
                .size_full()
                .top_0()
                .left_0()
                .bg(gpui::hsla(0., 0., 0., 0.55))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(520.))
                        .rounded_xl()
                        .border_1()
                        .border_color(YELLOW)
                        .bg(BG_PANEL)
                        .p_4()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(div().text_color(YELLOW).child(SharedString::from(format!("⚠ 权限确认 — manual 模式：{tool}"))))
                                .child(
                                    div()
                                        .rounded_md()
                                        .bg(BG_WINDOW)
                                        .p_2()
                                        .font_family("Menlo")
                                        .text_size(px(11.5))
                                        .text_color(TEXT_2)
                                        .child(args),
                                )
                                .child(
                                    div().flex().gap_2().child(
                                        div()
                                            .id("perm-allow")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(BORDER_STRONG)
                                            .text_color(TEXT_1)
                                            .text_size(px(12.))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(BG_HOVER))
                                            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                                this.on_permission(true, cx);
                                            }))
                                            .child("允许（1）"),
                                    ),
                                ),
                        ),
                )
        });

        div()
            .key_context("AppRoot")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::on_cycle_mode))
            .on_action(cx.listener(Self::on_palette_up))
            .on_action(cx.listener(Self::on_palette_down))
            .on_action(cx.listener(Self::on_palette_complete))
            .on_action(cx.listener(Self::on_close_palette))
            .on_action(cx.listener(Self::on_new_chat))
            .on_action(cx.listener(Self::on_open_settings))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(BG_MAIN)
            .text_color(TEXT_1)
            .when_some(perm_overlay, |d, ov| d.child(ov))
            // 顶栏
            .child(
                div()
                    .h_11()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .border_b_1()
                    .border_color(BORDER)
                    .bg(BG_MAIN)
                    .child(badge("🧠 glm-4.7-multimodal", TEXT_2))
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .border_1()
                            .border_color(BORDER)
                            .bg(BG_PANEL)
                            .font_family("Menlo")
                            .text_size(px(11.))
                            .text_color(TEXT_3)
                            .child("~/developCode/self/mcode"),
                    )
                    .child(
                        div()
                            .id("mode-badge")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                this.mode = this.perms.cycle_mode();
                                cx.notify();
                            }))
                            .child(badge(mode_label, mc)),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("btn-settings")
                            .cursor_pointer()
                            .px_2()
                            .text_size(px(12.))
                            .text_color(TEXT_2)
                            .on_click(cx.listener(|_, _: &gpui::ClickEvent, _, _| {}))
                            .child("⚙ 设置  🧩 扩展"),
                    ),
            )
            // 主体：侧栏 + 消息流
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        // 侧栏
                        div()
                            .w(px(232.))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .bg(BG_SIDE)
                            .border_r_1()
                            .border_color(BORDER)
                            .child(
                                div().px_3().pt_3().child(
                                    div()
                                        .id("btn-new")
                                        .w_full()
                                        .rounded_md()
                                        .bg(ACCENT)
                                        .py_1p5()
                                        .text_center()
                                        .text_size(px(12.5))
                                        .text_color(rgb(0xffffff))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                            this.messages.clear();
                                            this.queue.clear();
                                            this.running = false;
                                            cx.notify();
                                        }))
                                        .child("＋ 新建会话"),
                                ),
                            )
                            .child(
                                div().px_3().pt_3().pb_1().text_size(px(10.5)).text_color(TEXT_3).child("当前目录会话"),
                            )
                            .child(
                                div().px_2().flex().flex_col().children(self.sessions.iter().map(|(t, n)| {
                                    div()
                                        .px_2()
                                        .py_1p5()
                                        .rounded_sm()
                                        .text_size(px(12.))
                                        .text_color(TEXT_2)
                                        .flex()
                                        .child(t.clone())
                                        .child(div().ml_auto().text_color(TEXT_3).text_size(px(10.5)).child(n.clone()))
                                })),
                            )
                            .child(div().flex_1())
                            .child(
                                div().mx_3().mb_3().rounded_md().border_1().border_color(BORDER).bg(BG_PANEL).p_2()
                                    .child(div().flex().justify_between().text_size(px(10.5)).text_color(TEXT_3)
                                        .child("上下文占用").child("61k / 160k"))
                                    .child(div().mt_1().h_1().rounded_full().bg(BG_ACTIVE)
                                        .child(div().w_2_5().h_1().rounded_full().bg(ACCENT)))
                                    .child(div().mt_1().text_size(px(9.5)).text_color(TEXT_3).child("38% · /context")),
                            ),
                    )
                    .child(
                        // 主列
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                // 消息滚动区
                                div()
                                    .id("messages")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .px_5()
                                    .py_3()
                                    .children(message_list),
                            )
                            .child(queue_banner)
                            .child(
                                // Composer
                                div().px_5().pb_3()
                                    .when(palette_open, |d| {
                                        d.child(
                                            div()
                                                .id("palette")
                                                .rounded_lg()
                                                .border_1()
                                                .border_color(BORDER_STRONG)
                                                .bg(BG_PANEL)
                                                .overflow_hidden()
                                                .max_h(px(260.))
                                                .overflow_y_scroll()
                                                .children(palette_items),
                                        )
                                    })
                                    .child(
                                        div()
                                            .rounded_lg()
                                            .border_1()
                                            .border_color(BORDER_STRONG)
                                            .bg(BG_PANEL)
                                            .overflow_hidden()
                                            .child(self.input.clone())
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .px_3()
                                                    .pb_2()
                                                    .text_size(px(10.5))
                                                    .text_color(TEXT_3)
                                                    .child("🖼 附件")
                                                    .child(div().flex_1())
                                                    .child("⏎ 发送 · / 指令 · @ 文件 · ⇧Tab 模式 · ESC 打断"),
                                            ),
                                    ),
                            ),
                    ),
            )
    }
}
