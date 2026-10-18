//! MCode_Desktop — GPUI 桌面端入口

mod app;
mod chat_view;
mod file_explorer;
mod runtime;
mod session_ops;
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
    Application::new().run(|cx: &mut gpui::App| {
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

        let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("MCode Desktop".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        cx.open_window(options, |_, cx| {
            cx.new(|cx| App::new(cx))
        })
        .unwrap();
        cx.activate(true);
    });
}

/// ti 模块别名（KeyBinding 列表内引用 text_input 的动作类型）
use text_input as ti;
