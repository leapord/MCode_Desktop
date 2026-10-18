//! chat 布局渲染 — 布局规范 = docs/prototype/screens/chat.html：
//! 侧栏（会话/占用）+ 顶栏（模型/路径/模式/goal 徽章）+ 消息流（工具卡/diff/Notice）+ Composer。
//! 共享渲染小件（badge / tool_icon）也在这里，供 views.rs 的页面视图复用。

use gpui::{
    div, prelude::*, px, rgb, AnyElement, Context, Div, Focusable, InteractiveElement,
    StatefulInteractiveElement, SharedString, Window,
};
use mcode_core::agent::CONTEXT_LIMIT_TOKENS;
use mcode_core::permission::Decision;
use mcode_core::agent::context::MAX_GOAL_ROUNDS;

use crate::app::{trunc, App, ChatMsg, View};
use crate::theme::*;

/// 渲染分发：Onboarding / Settings / Extensions / Sessions / Chat
pub(crate) fn render_app(app: &mut App, window: &mut Window, cx: &mut Context<App>) -> AnyElement {
    match app.view {
        View::Onboarding => crate::views::render_onboarding(app, cx),
        View::Settings => crate::views::render_settings(app, cx),
        View::Extensions => crate::views::render_extensions(app, cx),
        View::Sessions => crate::views::render_sessions(app, cx),
        View::Chat => render_chat(app, window, cx),
    }
}

fn render_chat(app: &mut App, _window: &mut Window, cx: &mut Context<App>) -> AnyElement {
    let mc = mode_color(app.mode);
    let model_label: SharedString =
        format!("🧠 {}", app.cfg.active_model().unwrap_or_else(|| "未选择模型".into())).into();
    let cwd_label: SharedString = trunc(&app.cwd.to_string_lossy(), 46).into();
    let mode_label: SharedString = format!("⇥ {}", app.mode.label()).into();
    let goal_badge = app.goal.as_ref().map(|g| {
        let label: SharedString =
            format!("🎯 {g}（{r}/{max}）", r = app.goal_rounds, max = MAX_GOAL_ROUNDS).into();
        badge(label, YELLOW)
    });

    // 消息流
    let message_list: Vec<_> = app.messages.iter().map(|m| render_msg(m)).collect();

    // 补全面板
    let palette_open = app.completion.is_open() && !app.candidates.is_empty();
    let palette_items: Vec<_> = app
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let sel = i == app.completion.selected;
            div()
                .px_3()
                .py_1()
                .text_size(px(12.))
                .flex()
                .gap_2()
                .when(sel, |d| d.bg(rgb(0x3a2a24)))
                .child(
                    div()
                        .text_color(if sel { ACCENT } else { TEXT_1 })
                        .font_family("Menlo")
                        .child(c.name.clone()),
                )
                .when(!c.desc.is_empty(), |d| {
                    d.child(div().text_color(TEXT_3).child(c.desc.clone()))
                })
        })
        .collect();

    // 排队插话
    let queue_len = app.queue.len();
    let queue_banner = if queue_len > 0 {
        div().px_6().pb_1().child(
            div()
                .rounded_full()
                .border_1()
                .border_color(BLUE)
                .text_color(BLUE)
                .px_3()
                .py_1()
                .text_size(px(11.5))
                .child(format!("📥 {} 条排队插话", queue_len)),
        )
    } else {
        div()
    };

    // 权限弹窗（允许 / 总是允许 / 拒绝 — respond oneshot 回传 runner）
    let perm_overlay = app.pending_permission.as_ref().map(|p| {
        let tool: SharedString = p.tool.clone().into();
        let args: SharedString = p.args_summary.clone().into();
        let mode_label: SharedString = app.mode.label().into();
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
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_color(YELLOW)
                            .child(SharedString::from(format!(
                                "⚠ 权限确认 — {mode_label} 模式：{tool}"
                            ))),
                    )
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
                        div().flex().gap_2()
                            .child(perm_btn("允许", BORDER_STRONG, TEXT_1, cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                this.resolve_permission(Decision::Allow, cx);
                            })))
                            .child(perm_btn("总是允许", GREEN, GREEN, cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                this.resolve_permission(Decision::AllowAlways, cx);
                            })))
                            .child(perm_btn("拒绝", RED, RED, cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                this.resolve_permission(Decision::Deny, cx);
                            }))),
                    ),
            )
    });

    div()
        .key_context("AppRoot")
        .track_focus(&app.focus_handle(cx))
        .on_action(cx.listener(|this, _: &crate::app::CycleMode, _, cx| {
            this.cycle_mode();
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &crate::app::PaletteUp, _, cx| {
            if this.completion.is_open() {
                this.completion.move_up(this.candidates.len());
            } else if let Some(t) = this.history_recall_up(&this.input.read(cx).text()) {
                this.input.update(cx, |i, cx| i.set_text(&t, cx));
            }
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &crate::app::PaletteDown, _, cx| {
            if this.completion.is_open() {
                this.completion.move_down(this.candidates.len());
            } else if let Some(t) = this.history_recall_down() {
                this.input.update(cx, |i, cx| i.set_text(&t, cx));
            }
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &crate::app::PaletteComplete, _, cx| {
            if let Some(c) = this.candidates.get(this.completion.selected).cloned() {
                let text = this.input.read(cx).text();
                let new_text = this.completion.apply(&text, &c.name);
                this.input.update(cx, |inp, cx| inp.set_text(&new_text, cx));
            }
            this.completion.close();
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &crate::app::ClosePalette, _, cx| {
            this.interrupt(cx);
        }))
        .on_action(cx.listener(|this, _: &crate::app::NewChat, _, cx| {
            this.new_chat();
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &crate::app::OpenSettings, _, cx| {
            this.view = View::Settings;
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &crate::app::OpenExtensions, _, cx| {
            this.view = View::Extensions;
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &crate::app::OpenSessions, _, cx| {
            this.view = View::Sessions;
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &crate::app::BackToChat, _, cx| {
            this.view = View::Chat;
            cx.notify();
        }))
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
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .border_b_1()
                .border_color(BORDER)
                .bg(BG_MAIN)
                .child(badge(model_label, TEXT_2))
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
                        .child(cwd_label),
                )
                .child(
                    div()
                        .id("mode-badge")
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                            this.cycle_mode();
                            cx.notify();
                        }))
                        .child(badge(mode_label, mc)),
                )
                .children(goal_badge)
                .child(div().flex_1())
                .child(
                    div()
                        .id("btn-explorer-toggle")
                        .cursor_pointer()
                        .px_2()
                        .text_size(px(12.))
                        .text_color(if app.explorer_visible { ACCENT } else { TEXT_2 })
                        .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                            this.explorer_visible = !this.explorer_visible;
                            cx.notify();
                        }))
                        .child("🗂 文件"),
                )
                .child(
                    div()
                        .id("btn-settings")
                        .cursor_pointer()
                        .px_2()
                        .text_size(px(12.))
                        .text_color(TEXT_2)
                        .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                            this.view = View::Settings;
                            cx.notify();
                        }))
                        .child("⚙ 设置"),
                )
                .child(
                    div()
                        .id("btn-extensions")
                        .cursor_pointer()
                        .px_2()
                        .text_size(px(12.))
                        .text_color(TEXT_2)
                        .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                            this.view = View::Extensions;
                            cx.notify();
                        }))
                        .child("🧩 扩展"),
                ),
        )
        // 主体：侧栏 + 消息流
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(if app.sidebar_collapsed {
                    render_sidebar_collapsed(app, cx)
                } else {
                    render_sidebar(app, cx)
                })
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
                                        .child(app.input.clone())
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .px_3()
                .items_center()
                                                .pb_2()
                                                .text_size(px(10.5))
                                                .text_color(TEXT_3)
                                                .child("⏎ 发送 · / 指令 · @ 文件 · ⇧Tab 模式 · ESC 打断"),
                                        ),
                                ),
                        ),
                )
                .when(app.explorer_visible, |d| {
                    d.child(crate::file_explorer::render_file_explorer(app, cx))
                }),
        )
        .into_any_element()
}

// ---------- 消息渲染 ----------

fn render_msg(m: &ChatMsg) -> AnyElement {
    match m {
        ChatMsg::User { text, refs } => div()
            .mb_3()
            .max_w(px(860.))
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
            .max_w(px(860.))
            .text_color(TEXT_1)
            .child(text.clone()),
        ChatMsg::Thinking { text } => div()
            .mb_2()
            .max_w(px(860.))
            .rounded_sm()
            .border_1()
            .border_color(BORDER)
            .bg(BG_PANEL)
            .px_2()
            .py_1()
            .text_size(px(11.5))
            .text_color(TEXT_3)
            .child(SharedString::from(format!(
                "💭 思考过程 · {} 字符 · 已折叠",
                text.chars().count()
            ))),
        ChatMsg::ToolCard { name, args, summary, running, success } => {
            let (icon, color) = tool_icon(name);
            let status: SharedString = if *running {
                "◌ 运行中".into()
            } else if *success {
                format!("✓ {summary}").into()
            } else {
                format!("✗ {summary}").into()
            };
            div()
                .mb_2()
                .max_w(px(860.))
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
                .child(div().text_color(TEXT_1).child(name.clone()))
                .child(div().text_color(TEXT_3).child(args.clone()))
                .child(
                    div()
                        .ml_auto()
                        .text_color(if *running {
                            YELLOW
                        } else if *success {
                            GREEN
                        } else {
                            RED
                        })
                        .child(status),
                )
        }
        ChatMsg::Diff { path, lines } => {
            let mut wrap = div()
                .mb_2()
                .max_w(px(860.))
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
            for l in lines {
                let (bg, fg) = if l.hunk {
                    (BG_WINDOW, PURPLE)
                } else if l.add {
                    (rgb(0x1b2a1e), rgb(0x9fd9a3))
                } else if l.del {
                    (rgb(0x2d1b1b), rgb(0xeb9a9a))
                } else {
                    (BG_WINDOW, TEXT_3)
                };
                let prefix = if l.add { "+" } else if l.del { "-" } else { " " };
                wrap = wrap.child(
                    div()
                        .bg(bg)
                        .px_3()
                        .text_size(px(11.5))
                        .font_family("Menlo")
                        .text_color(fg)
                        .child(SharedString::from(format!("{prefix}{}", l.text))),
                );
            }
            wrap
        }
        ChatMsg::Notice { text, error } => div()
            .mb_2()
            .max_w(px(860.))
            .rounded_sm()
            .border_1()
            .border_color(if *error { RED } else { BORDER })
            .bg(if *error { rgb(0x2d1b1b) } else { BG_PANEL })
            .px_2()
            .py_1()
            .text_size(px(11.5))
            .text_color(if *error { RED } else { TEXT_2 })
            .child(text.clone()),
        ChatMsg::Running => div()
            .mb_3()
            .text_color(ACCENT)
            .child("◌ 运行中…  （ESC 打断）"),
    }
    .into_any_element()
}

// ---------- 共享渲染小件 ----------

pub(crate) fn badge(label: impl Into<SharedString>, color: gpui::Rgba) -> Div {
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

/// 权限弹窗按钮
fn perm_btn(
    label: &str,
    border: gpui::Rgba,
    text: gpui::Rgba,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<Div> {
    div()
        .id(SharedString::from(format!("perm-{}", label)))
        .px_3()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(border)
        .text_color(text)
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|s| s.bg(BG_HOVER))
        .on_click(on_click)
        .child(label.to_string())
}

/// 侧栏折叠/展开小按钮（« / »）
fn collapse_btn(cx: &mut Context<App>, expanded: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id("btn-collapse")
        .px_1p5()
        .py_0p5()
        .rounded_sm()
        .text_size(px(12.))
        .text_color(TEXT_3)
        .cursor_pointer()
        .hover(|s| s.bg(BG_HOVER))
        .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
            this.sidebar_collapsed = !expanded;
            cx.notify();
        }))
        .child(if expanded { "«" } else { "»" })
}

/// 工具类别 → (图标, 语义色)
pub(crate) fn tool_icon(name: &str) -> (&'static str, gpui::Rgba) {
    if name == "task" {
        ("⚡", PURPLE)
    } else if name == "bash" {
        ("▶", YELLOW)
    } else if matches!(name, "read_file" | "glob" | "grep" | "ls") {
        ("📄", BLUE)
    } else if matches!(name, "edit_file" | "write_file") {
        ("✏️", GREEN)
    } else if name.starts_with("mcp__") {
        ("🔗", BLUE)
    } else {
        ("✏️", GREEN)
    }
}

fn ctx_k(n: usize) -> String {
    format!("{}k", n / 1000)
}

// ---------- 左侧功能区（可折叠） ----------

/// 展开态侧栏（原布局 + 头部折叠按钮）
fn render_sidebar(app: &mut App, cx: &mut Context<App>) -> gpui::Div {
    let ctx = app.ctx_tokens_display();
    let pct = (ctx as f32 / CONTEXT_LIMIT_TOKENS as f32 * 100.).min(100.);
    div()
        .w(px(232.))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .bg(BG_SIDE)
        .border_r_1()
        .border_color(BORDER)
        .child(
            div().px_3().pt_3().flex().items_center().gap_1().child(
                div()
                    .id("btn-new")
                    .flex_1()
                    .rounded_md()
                    .bg(ACCENT)
                    .py_1p5()
                    .text_center()
                    .text_size(px(12.5))
                    .text_color(rgb(0xffffff))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                        this.new_chat();
                        cx.notify();
                    }))
                    .child("＋ 新建会话"),
            )
            .child(collapse_btn(cx, true)),
        )
        .child(
            div().px_3().pt_3().pb_1().text_size(px(10.5)).text_color(TEXT_3).child("当前目录会话"),
        )
        .child(
            div().px_2().flex().flex_col().children(app.sessions.iter().take(8).map(|m| {
                let title: SharedString = trunc(&m.title, 16).into();
                let meta: SharedString = format!("{}轮", m.turns).into();
                let id = m.id.clone();
                div()
                    .id(SharedString::from(format!("sess-{}", m.id)))
                    .px_2()
                    .py_1p5()
                    .rounded_sm()
                    .text_size(px(12.))
                    .text_color(TEXT_2)
                    .cursor_pointer()
                    .hover(|s| s.bg(BG_HOVER))
                    .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                        this.resume_session(&id, cx);
                    }))
                    .flex()
                    .child(title)
                    .child(div().ml_auto().text_color(TEXT_3).text_size(px(10.5)).child(meta))
            })),
        )
        .child(div().flex_1())
        .child(
            div().mx_3().mb_3().rounded_md().border_1().border_color(BORDER).bg(BG_PANEL).p_2()
                .child(
                    div().flex().justify_between().text_size(px(10.5)).text_color(TEXT_3)
                        .child("上下文占用")
                        .child(SharedString::from(format!("{}/160k", ctx_k(ctx)))),
                )
                .child(div().mt_1().h_1().rounded_full().bg(BG_ACTIVE).child(
                    div().h_1().rounded_full().bg(ACCENT).w(gpui::relative(pct / 100.)),
                ))
                .child(div().mt_1().text_size(px(9.5)).text_color(TEXT_3)
                    .child(SharedString::from(format!("{pct:.0}% · /context")))),
        )
}

/// 折叠态侧栏：40px 图标条（新建会话 + 设置/扩展/会话 + 展开）
fn render_sidebar_collapsed(_app: &mut App, cx: &mut Context<App>) -> gpui::Div {
    let icon_btn = |id: &'static str, icon: &'static str| -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .w_full()
            .py_2()
            .text_center()
            .text_size(px(14.))
            .text_color(TEXT_2)
            .cursor_pointer()
            .hover(|s| s.bg(BG_HOVER))
            .child(icon.to_string())
    };
    div()
        .w(px(40.))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .items_center()
        .bg(BG_SIDE)
        .border_r_1()
        .border_color(BORDER)
        .py_2()
        .gap_1()
        .child(
            icon_btn("c-new", "＋")
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                    this.new_chat();
                    cx.notify();
                })),
        )
        .child(
            icon_btn("c-sessions", "🗂")
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                    this.view = View::Sessions;
                    cx.notify();
                })),
        )
        .child(
            icon_btn("c-ext", "🧩")
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                    this.view = View::Extensions;
                    cx.notify();
                })),
        )
        .child(
            icon_btn("c-settings", "⚙")
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                    this.view = View::Settings;
                    cx.notify();
                })),
        )
        .child(div().flex_1())
        .child(
            icon_btn("c-expand", "»")
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                    this.sidebar_collapsed = false;
                    cx.notify();
                })),
        )
}
