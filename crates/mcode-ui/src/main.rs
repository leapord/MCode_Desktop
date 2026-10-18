// MCode_Desktop — AI coding agent desktop app
// Copyright (c) 2026 Materin-tech. All rights reserved.
// SPDX-License-Identifier: MIT

//! MCode_Desktop — GPUI 桌面端入口

mod app;
#[cfg(test)]
mod fs_menu_tests;
#[cfg(test)]
mod interaction_tests;
mod markdown_view;
mod msg;
mod chat_view;
mod context_menu;
mod file_explorer;
mod runtime;
mod session_ops;
mod sidebar_view;
mod theme;
mod text_input;
mod views;
mod workspace_picker;

use app::App;
use gpui::{
    actions, prelude::*, px, size, Application, Bounds, KeyBinding, TitlebarOptions,
    WindowBounds, WindowOptions,
};

actions!(desktop, [Quit]);

fn main() {
    // macOS 标准行为：关窗保留进程，Dock 图标 reopen（applicationShouldHandleReopen）
    // 重建窗口——gpui 0.2 经 Application::on_reopen 暴露该回调
    let app = Application::new();
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            let root = spawn_main_window(cx);
            root.update(cx, |app, window, cx| focus_composer(app, window, cx)).ok();
        }
        cx.activate(true);
    });
    app.run(|cx: &mut gpui::App| {
        cx.bind_keys([
            // TextInput 编辑动作
            KeyBinding::new("backspace", ti::Backspace, None),
            KeyBinding::new("delete", ti::Delete, None),
            KeyBinding::new("left", ti::Left, None),
            KeyBinding::new("right", ti::Right, None),
            KeyBinding::new("shift-left", ti::SelectLeft, None),
            KeyBinding::new("shift-right", ti::SelectRight, None),
            KeyBinding::new("cmd-a", ti::SelectAll, None),
            KeyBinding::new("cmd-v", ti::Paste, None),
            KeyBinding::new("cmd-c", ti::Copy, None),
            KeyBinding::new("cmd-x", ti::Cut, None),
            KeyBinding::new("home", ti::Home, None),
            KeyBinding::new("end", ti::End, None),
            KeyBinding::new("enter", ti::Submit, None),
            // 应用动作
            KeyBinding::new("shift-tab", app::CycleMode, None),
            KeyBinding::new("up", app::PaletteUp, None),
            KeyBinding::new("down", app::PaletteDown, None),
            KeyBinding::new("tab", app::PaletteComplete, None),
            KeyBinding::new("escape", app::ClosePalette, None),
            KeyBinding::new("cmd-n", app::NewChat, None),
            KeyBinding::new("cmd-,", app::OpenSettings, None),
            KeyBinding::new("cmd-e", app::OpenExtensions, None),
            KeyBinding::new("cmd-j", app::OpenSessions, None),
            // 文件浏览器键上下文（焦点在浏览器时优先于全局绑定）
            KeyBinding::new("cmd-c", file_explorer::FsCopy, Some("FileExplorer")),
            KeyBinding::new("cmd-x", file_explorer::FsCut, Some("FileExplorer")),
            KeyBinding::new("cmd-v", file_explorer::FsPaste, Some("FileExplorer")),
            KeyBinding::new("cmd-backspace", file_explorer::FsDelete, Some("FileExplorer")),
            KeyBinding::new("up", file_explorer::FsSelUp, Some("FileExplorer")),
            KeyBinding::new("down", file_explorer::FsSelDown, Some("FileExplorer")),
            KeyBinding::new("right", file_explorer::FsToggleDir, Some("FileExplorer")),
            KeyBinding::new("cmd-n", file_explorer::FsNewFile, Some("FileExplorer")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());

        // 全局句柄必须先初始化——spawn_main_window 会读写它（曾因缺失导致启动即崩）
        cx.set_global(MainWindowHandle::default());
        let root = spawn_main_window(cx);
        root.update(cx, |app, window, cx| focus_composer(app, window, cx)).ok();
        cx.activate(true);
    });
}

/// ti 模块别名（KeyBinding 列表内引用 text_input 的动作类型）
use text_input as ti;

/// 主窗口句柄全局（Dock reopen 重建用）
#[derive(Default)]
struct MainWindowHandle(std::cell::RefCell<Option<gpui::WindowHandle<App>>>);
impl gpui::Global for MainWindowHandle {}

/// 创建主窗口（启动与 Dock reopen 共用）
fn spawn_main_window(cx: &mut gpui::App) -> gpui::WindowHandle<App> {
    let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("MCode Desktop".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let root = cx.open_window(options, |_, cx| cx.new(|cx| App::new(cx))).unwrap();
    cx.global::<MainWindowHandle>().0.borrow_mut().replace(root.clone());
    root
}

/// 启动即聚焦 Composer:gpui 的按键 action 只沿焦点路径分发,无焦点时 Enter(Submit)
/// 根本到不了 TextInput——用户实测"输入后零响应"的根因即此(窗口打开后无任何元素持焦)。
fn focus_composer(app: &mut App, window: &mut gpui::Window, cx: &mut gpui::Context<App>) {
    let handle = app.input.read(cx).focus_handle.clone();
    handle.focus(window);
}
