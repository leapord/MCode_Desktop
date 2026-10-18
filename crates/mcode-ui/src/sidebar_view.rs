//! 左侧功能区渲染（可折叠：完整侧栏 / 40px 图标条）。

use gpui::{div, prelude::*, px, rgb, Context, InteractiveElement, SharedString, StatefulInteractiveElement};
use mcode_core::agent::CONTEXT_LIMIT_TOKENS;

use crate::app::{App, View};
use crate::msg::trunc;
use crate::theme::*;

/// 侧栏折叠/展开小按钮（« / »）
/// 根因注记（用户实测「点击无响应」）：接线形态与可用的顶栏按钮完全同构，非 gpui 命中测试问题——
/// gpui 0.2 on_click 语义 = mouse-down 与 mouse-up 都命中同一 hitbox（elements/div.rs
/// paint_mouse_listeners 的 pending_mouse_down 配对，is_hovered 仅被后续 paint 的 BlockMouse
/// hitbox 遮挡，本布局无遮挡）。真实根因是 handler 逻辑反转：旧实现
/// `sidebar_collapsed = !expanded`，展开态调用 collapse_btn(cx, true) 恒置 false，
/// 即点击 « 把状态写回原值，视觉零变化。改为直接赋目标值，并把命中区扩到 ≥24×24
/// （原 px_1p5+py_0p5 命中区约 20×16，位移 1px 即不判 click，次要因素）。
fn collapse_btn(cx: &mut Context<App>, collapse_to: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id("btn-collapse")
        .min_w(px(24.))
        .min_h(px(24.))
        .p_1p5()
        .rounded_sm()
        .text_size(px(12.))
        .text_color(TEXT_3)
        .cursor_pointer()
        .hover(|s| s.bg(BG_HOVER))
        .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
            this.sidebar_collapsed = collapse_to;
            cx.notify();
        }))
        .child(if collapse_to { "«" } else { "»" })
}



// ---------- 左侧功能区（可折叠） ----------

/// 展开态侧栏（原布局 + 头部折叠按钮）
pub(crate) fn render_sidebar(app: &mut App, cx: &mut Context<App>) -> gpui::Div {
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
pub(crate) fn render_sidebar_collapsed(_app: &mut App, cx: &mut Context<App>) -> gpui::Div {
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

fn ctx_k(n: usize) -> String {
    format!("{}k", n / 1000)
}
