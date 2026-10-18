//! 页面视图 — Settings / Extensions / Sessions / Onboarding 四视图渲染
//! （chat 布局在 chat_view.rs；文件超 800 行时按规格拆分）。
//! 布局规范 = docs/prototype/screens/{settings,extensions,sessions,onboarding}.html。

use gpui::{
    div, prelude::*, px, rgb, AnyElement, InteractiveElement, SharedString, StatefulInteractiveElement,
};
use mcode_core::permission::Mode;

use crate::app::{trunc, App, View};
use crate::chat_view::badge;
use crate::session_ops::{lang_label, lang_next};
use crate::theme::*;

const MODES: [Mode; 4] = [Mode::Passon, Mode::Manual, Mode::Auto, Mode::Plan];

fn mode_hint(m: Mode) -> &'static str {
    m.hint()
}

// ---------- Onboarding ----------

pub fn render_onboarding(app: &mut App, cx: &mut gpui::Context<App>) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(BG_WINDOW)
        .text_color(TEXT_1)
        .child(
            div()
                .w(px(620.))
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .rounded_xl()
                        .border_1()
                        .border_color(BORDER_STRONG)
                        .bg(BG_PANEL)
                        .p_6()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(div().text_size(px(20.)).child("👋 欢迎使用 MCode Desktop"))
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(TEXT_2)
                                .child("首次启动需要配置模型供应商。所有配置写入 ~/.mcode/config.json，与 CLI 版完全互通。"),
                        )
                        .child(field("Provider 名称", app.ob_name.clone()))
                        .child(field("Base URL", app.ob_url.clone()))
                        .child(field("API Key（环境变量名，不落明文）", app.ob_key.clone()))
                        .child(field("可用模型（逗号分隔）", app.ob_models.clone()))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(TEXT_3)
                                .child("apiKey 只存环境变量名；带 multimodal 标记的模型才接收图片附件（可在 desktop.json 扩展配置）。"),
                        )
                        .child(
                            div().flex().justify_end().child(
                                div()
                                    .id("btn-onboard-save")
                                    .px_4()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(ACCENT)
                                    .text_color(rgb(0xffffff))
                                    .text_size(px(12.5))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                                        this.onboarding_save(cx);
                                    }))
                                    .child("保存并进入对话 →"),
                            ),
                        ),
                ),
        )
        .into_any_element()
}

fn field(label: &str, input: gpui::Entity<crate::text_input::TextInput>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_size(px(12.)).text_color(TEXT_2).child(label.to_string()))
        .child(
            div()
                .rounded_md()
                .border_1()
                .border_color(BORDER_STRONG)
                .bg(rgb(0x222228))
                .overflow_hidden()
                .child(input),
        )
}

// ---------- Settings ----------

pub fn render_settings(app: &mut App, cx: &mut gpui::Context<App>) -> AnyElement {
    let provider = app.cfg.active_provider().cloned();
    let desktop = mcode_core::config::DesktopConfig::load();
    let mut root = page_scaffold(
        "⚙ 设置中心",
        "~/.mcode/config.json · 唯一事实源（CLI 双端互通）",
        cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
            this.view = View::Chat;
            cx.notify();
        }),
    )
    .flex()
    .flex_col()
    .gap_5()
    .p_6();

    // 供应商卡片 + 模型列表（点击切换 → save + rebuild harness）
    root = root.child(
        div().flex().flex_col().gap_2().child(div().text_size(px(15.)).child("🔌 模型供应商"))
            .children(provider.map(|p| {
                div()
                    .rounded_lg()
                    .border_1()
                    .border_color(ACCENT)
                    .bg(BG_PANEL)
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div().flex().items_center().gap_2()
                            .child(div().text_size(px(13.5)).child(p.name.clone()))
                            .child(badge("当前激活", ACCENT)),
                    )
                    .child(
                        div().font_family("Menlo").text_size(px(11.5)).text_color(TEXT_3)
                            .child(SharedString::from(format!("{} · key: ${}", p.base_url, p.api_key_env))),
                    )
            }))
            .children(app.cfg.active_provider().map(|p| {
                div().flex().flex_col().gap_1().children(p.models.iter().enumerate().map(|(i, m)| {
                    let active = app.cfg.active_model().as_deref() == Some(m.as_str());
                    let model = m.clone();
                    div()
                        .id(SharedString::from(format!("model-{i}")))
                        .px_3()
                        .py_1p5()
                        .rounded_sm()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(12.))
                        .font_family("Menlo")
                        .text_color(if active { ACCENT } else { TEXT_1 })
                        .cursor_pointer()
                        .hover(|s| s.bg(BG_HOVER))
                        .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                            if this.set_model(&model) {
                                this.save_cfg_and_rebuild(cx);
                                this.push_notice(format!("✓ 模型 → {model}"), false);
                            }
                            cx.notify();
                        }))
                        .child(if active { format!("▸ {m} · 使用中") } else { m.clone() })
                }))
            })),
    );

    // 语言 / 默认模式 / PUA
    root = root
        .child(
            div().flex().flex_col().gap_2().child(div().text_size(px(15.)).child("🌐 语言环境（/language）"))
                .child(
                    div()
                        .id("lang-cycle")
                        .w(px(280.))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .border_1()
                        .border_color(BORDER_STRONG)
                        .bg(BG_PANEL)
                        .cursor_pointer()
                        .hover(|s| s.bg(BG_HOVER))
                        .text_size(px(12.))
                        .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                            this.cfg.language = lang_next(this.cfg.language);
                            this.save_cfg_and_rebuild(cx);
                            cx.notify();
                        }))
                        .child(SharedString::from(lang_label(app.cfg.language).to_string())),
                ),
        )
        .child(
            div().flex().flex_col().gap_2().child(div().text_size(px(15.)).child("🛡 默认权限模式（会话内 shift+tab 临时切换）"))
                .child(
                    div().flex().gap_2().children(MODES.map(|m| {
                        let sel = app.cfg.default_mode == m;
                        div()
                            .id(SharedString::from(format!("mode-{}", m.label())))
                            .flex_1()
                            .p_3()
                            .rounded_md()
                            .border_1()
                            .border_color(if sel { mode_color(m) } else { BORDER_STRONG })
                            .bg(BG_PANEL)
                            .cursor_pointer()
                            .hover(|s| s.bg(BG_HOVER))
                            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                                this.set_default_mode(m, cx);
                                cx.notify();
                            }))
                            .child(
                                div().text_size(px(13.)).child(SharedString::from(format!(
                                    "● {}",
                                    m.label()
                                ))),
                            )
                            .child(div().text_size(px(11.)).text_color(TEXT_3).child(mode_hint(m)))
                    })),
                ),
        )
        .child(
            div().flex().items_center().gap_2().child(div().text_size(px(15.)).child("🧘 PUA 收敛纪律"))
                .child(
                    div()
                        .id("pua-toggle")
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .border_1()
                        .border_color(if app.cfg.pua_always_on { GREEN } else { BORDER_STRONG })
                        .bg(BG_PANEL)
                        .cursor_pointer()
                        .text_size(px(12.))
                        .text_color(if app.cfg.pua_always_on { GREEN } else { TEXT_2 })
                        .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                            let on = this.cfg.pua_always_on;
                            this.set_pua(!on, cx);
                            cx.notify();
                        }))
                        .child(if app.cfg.pua_always_on { "已开启（/pua:off 关闭）" } else { "已关闭（/pua:on 开启）" }),
                ),
        );

    // 权限规则（desktop.json 来源）
    root = root.child(
        div().flex().flex_col().gap_2().child(div().text_size(px(15.)).child("🔓 权限规则（~/.mcode/desktop.json）"))
            .child(
                if desktop.permission_rules.is_empty() {
                    div().text_size(px(12.)).text_color(TEXT_3).child("暂无规则（allow/deny 子串匹配，编辑 desktop.json 后重启生效）")
                } else {
                    div().flex().flex_col().gap_1().children(desktop.permission_rules.iter().map(|r| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1p5()
                            .rounded_sm()
                            .bg(BG_PANEL)
                            .text_size(px(11.5))
                            .font_family("Menlo")
                            .child(div().text_color(TEXT_1).child(r.tool.clone()))
                            .child(div().text_color(TEXT_3).child(r.pattern.clone()))
                            .child(div().ml_auto().text_color(if r.allow { GREEN } else { RED })
                                .child(if r.allow { "ALLOW" } else { "DENY" }))
                    }))
                },
            ),
    );

    // 重新运行引导
    root = root.child(
        div().flex().child(
            div()
                .id("btn-rerun-onboarding")
                .px_3()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(BORDER_STRONG)
                .bg(BG_PANEL)
                .cursor_pointer()
                .text_size(px(12.))
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                    this.view = View::Onboarding;
                    cx.notify();
                }))
                .child("⟲ 重新运行引导"),
        ),
    );

    root.into_any_element()
}

// ---------- Extensions ----------

pub fn render_extensions(app: &mut App, cx: &mut gpui::Context<App>) -> AnyElement {
    let desktop = mcode_core::config::DesktopConfig::load();
    let mut root = page_scaffold(
        "🧩 扩展生态",
        "MCP / hooks · 配置来源 ~/.mcode/desktop.json",
        cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
            this.view = View::Chat;
            cx.notify();
        }),
    )
    .flex()
    .flex_col()
    .gap_5()
    .p_6();

    // MCP servers：名称/命令/工具数（harness.mcp_report）+ 失败原因
    let mut mcp_rows: Vec<gpui::Div> = vec![];
    for cfg in &desktop.mcp_servers {
        let report = app
            .harness
            .as_ref()
            .and_then(|h| h.mcp_report.iter().find(|(n, _)| n == &cfg.name));
        let (dot, state): (&'static str, SharedString) = match report {
            Some((_, Ok(n))) => ("🟢", format!("connected · 已注入 {n} 个工具").into()),
            Some((_, Err(e))) => ("🔴", format!("连接失败：{}", crate::app::trunc(&format!("{e:#}"), 60)).into()),
            None => ("⚪", "已停用".into()),
        };
        let name: SharedString = cfg.name.clone().into();
        let meta: SharedString = format!("{} · {}", cfg.command, cfg.args.join(" ")).into();
        mcp_rows.push(
            div()
                .flex()
                .items_center()
                .gap_3()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(BORDER)
                .bg(BG_PANEL)
                .child(div().text_size(px(14.)).child(dot))
                .child(
                    div().flex().flex_col()
                        .child(div().text_size(px(12.5)).text_color(TEXT_1).child(name))
                        .child(div().font_family("Menlo").text_size(px(10.5)).text_color(TEXT_3).child(meta)),
                )
                .child(div().ml_auto().text_size(px(11.)).text_color(if matches!(report, Some((_, Ok(_)))) { GREEN } else { TEXT_3 })
                    .child(state)),
        );
    }
    root = root.child(
        div().flex().flex_col().gap_2()
            .child(div().text_size(px(15.)).child("🔗 MCP Servers（/mcp）"))
            .children(mcp_rows)
            .when(desktop.mcp_servers.is_empty(), |d| {
                d.child(div().text_size(px(12.)).text_color(TEXT_3).child("未配置 MCP server"))
            }),
    );

    // hooks：event / matcher / command
    root = root.child(
        div().flex().flex_col().gap_2()
            .child(div().text_size(px(15.)).child("🪝 Hooks（/hooks）"))
            .child(if desktop.hooks.is_empty() {
                div().text_size(px(12.)).text_color(TEXT_3).child("未配置 hooks")
            } else {
                div().flex().flex_col().gap_1().children(desktop.hooks.iter().map(|h| {
                    let event: SharedString = format!("{:?}", h.event).into();
                    let matcher: SharedString = h.matcher.clone().into();
                    let command: SharedString = crate::app::trunc(&h.command, 60).into();
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_1p5()
                        .rounded_sm()
                        .bg(BG_PANEL)
                        .text_size(px(11.5))
                        .font_family("Menlo")
                        .child(div().text_color(PURPLE).child(event))
                        .child(div().text_color(TEXT_3).child(matcher))
                        .child(div().text_color(TEXT_2).child(command))
                }))
            })
            .child(
                div().text_size(px(11.5)).text_color(TEXT_3)
                    .child("提示：编辑 ~/.mcode/desktop.json 后重启生效。MCP 工具与内置工具同一权限门控。"),
            ),
    );

    root.into_any_element()
}

// ---------- Sessions ----------

pub fn render_sessions(app: &mut App, cx: &mut gpui::Context<App>) -> AnyElement {
    let mut root = page_scaffold(
        "🗂 会话列表",
        "当前目录历史会话 · 点击恢复（/resume）",
        cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
            this.view = View::Chat;
            cx.notify();
        }),
    )
    .flex()
    .flex_col()
    .gap_2()
    .p_6();

    if app.sessions.is_empty() {
        root = root.child(
            div().text_size(px(12.)).text_color(TEXT_3).child("暂无会话记录（发送第一条消息后自动创建）"),
        );
    }
    root = root.children(app.sessions.iter().map(|m| {
        let title: SharedString = trunc(&m.title, 40).into();
        let meta: SharedString = format!("{}轮 · {} tokens · {}", m.turns, m.tokens_total, crate::app::trunc(&m.updated, 10)).into();
        let id = m.id.clone();
        div()
            .id(SharedString::from(format!("page-sess-{}", m.id)))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(BORDER)
            .bg(BG_PANEL)
            .cursor_pointer()
            .hover(|s| s.bg(BG_HOVER))
            .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                this.resume_session(&id, cx);
            }))
            .child(div().text_size(px(13.)).text_color(TEXT_1).child(title))
            .child(div().ml_auto().text_size(px(11.)).text_color(TEXT_3).child(meta))
    }));

    root.into_any_element()
}

// ---------- 页面骨架（顶部标题栏 + 返回按钮） ----------

fn page_scaffold(
    title: &str,
    sub: &str,
    on_back: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(BG_MAIN)
        .text_color(TEXT_1)
        .child(
            div()
                .h_11()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .border_b_1()
                .border_color(BORDER)
                .child(div().text_size(px(12.5)).child(title.to_string()))
                .child(div().text_size(px(11.)).text_color(TEXT_3).child(sub.to_string()))
                .child(div().flex_1())
                .child(
                    div()
                        .id("btn-back")
                        .px_2()
                        .cursor_pointer()
                        .text_size(px(12.))
                        .text_color(TEXT_2)
                        .hover(|s| s.bg(BG_HOVER))
                        .on_click(on_back)
                        .child("← 返回对话（ESC）"),
                ),
        )
        .id("page-root")
        .overflow_y_scroll()
}
