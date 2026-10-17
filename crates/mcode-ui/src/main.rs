//! MCode_Desktop — GPUI 桌面端入口

mod app;
mod theme;
mod text_input;

use app::App;
use gpui::{
    actions, px, size, Application, Bounds, KeyBinding, TitlebarOptions, WindowBounds,
    WindowOptions,
};
use text_input as ti;

actions!(desktop, [Quit]);

fn main() {
    Application::new().run(|cx: &mut gpui::App| {
        cx.bind_keys([
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
            KeyBinding::new("shift-tab", app::CycleMode, None),
            KeyBinding::new("up", app::PaletteUp, None),
            KeyBinding::new("down", app::PaletteDown, None),
            KeyBinding::new("tab", app::PaletteComplete, None),
            KeyBinding::new("escape", app::ClosePalette, None),
            KeyBinding::new("cmd-n", app::NewChat, None),
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
            let input = cx
                .new(|cx| TextInput {
                    focus_handle: cx.focus_handle(),
                    content: "".into(),
                    placeholder: "向 MCode 发送消息…  （/ 指令 · @ 文件）".into(),
                    selected_range: 0..0,
                    selection_reversed: false,
                    marked_range: None,
                    last_layout: None,
                    last_bounds: None,
                    is_selecting: false,
                })
                .unwrap();
            cx.new(|cx| App::new(cx, input))
        })
        .unwrap();
        cx.activate(true);
    });
}
