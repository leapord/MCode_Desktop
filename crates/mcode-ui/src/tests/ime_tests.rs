// MCode_Desktop — AI coding agent desktop app
// Copyright (c) 2026 Materin-tech. All rights reserved.
// SPDX-License-Identifier: MIT

//! IME 组词守卫与消息区滚动跟随的状态机测试：
//! - 组词期（marked text 未上屏）退格/回车必须交给输入法，不得删已上屏内容、不得误发送
//! - 上翻暂停跟随的三个恢复路径（新建/清空/切换会话）都要把跟随重置回贴底

use crate::views::text_input::{self as ti, TextInput};
use gpui::{AppContext, EntityInputHandler, TestAppContext};

fn make_input_window(cx: &mut TestAppContext) -> gpui::WindowHandle<TextInput> {
    cx.add_window(|_, cx| TextInput {
        focus_handle: cx.focus_handle(),
        content: "".into(),
        placeholder: "测试".into(),
        selected_range: 0..0,
        selection_reversed: false,
        marked_range: None,
        last_layout: None,
        last_bounds: None,
        is_selecting: false,
    })
}

/// 中文输入法组词期：退格/回车交给输入法，不删已上屏内容、不当发送；
/// 上屏（unmark）后恢复正常行为。
#[gpui::test]
fn ime_composition_guards_backspace_and_submit(cx: &mut TestAppContext) {
    let window = make_input_window(cx);
    window
        .update(cx, |input, window, cx| {
            input.set_text("已上屏", cx);

            // 模拟 IME 组词：光标处挂 marked text（未上屏）
            input.replace_and_mark_text_in_range(None, "ni", None, window, cx);
            assert_eq!(input.text(), "已上屏ni", "组词文本应临时拼入内容");
            assert!(input.marked_range.is_some(), "组词状态未置位");

            // 组词期：退格/回车属于输入法，应用侧守卫必须全部拦截
            input.backspace(&ti::Backspace, window, cx);
            assert_eq!(input.text(), "已上屏ni", "组词期退格不得删已上屏内容");
            input.submit(&ti::Submit, window, cx);
            assert_eq!(input.text(), "已上屏ni", "组词期回车不得当发送清空");

            // 上屏（unmark 后守卫解除）→ 重建已知状态，验证退格/回车恢复正常
            input.unmark_text(window, cx);
            assert!(input.marked_range.is_none(), "上屏后组词状态应清除");
            input.set_text("abc", cx);
            input.backspace(&ti::Backspace, window, cx);
            assert_eq!(input.text(), "ab", "上屏后退格应恢复正常删除");
            input.submit(&ti::Submit, window, cx);
            assert_eq!(input.text(), "", "非组词期回车发送后应清空");
        })
        .unwrap();
}

/// 上翻暂停跟随的三个恢复路径（新建/清空/切换会话）都要把跟随重置回贴底。
#[gpui::test]
fn scroll_follow_resets_on_new_chat_clear_and_switch(cx: &mut TestAppContext) {
    let mut c = crate::tests::interaction_tests::setup(cx, vec![]);
    c.app.update(cx, |a, cx| {
        // 1) 新建会话
        a.chat_follow = false;
        a.new_chat();
        assert!(a.chat_follow, "新建会话应恢复滚动跟随");

        // 2) 切换/重载会话（/clear 会结束会话，放最后验证）
        let id = a
            .session
            .as_ref()
            .expect("new_chat 后应有新会话")
            .meta
            .id
            .clone();
        a.chat_follow = false;
        a.resume_session(&id, cx);
        assert!(a.chat_follow, "切换会话应恢复滚动跟随");

        // 3) /clear 命令
        a.chat_follow = false;
        assert!(a.handle_command("/clear", cx));
        assert!(a.chat_follow, "/clear 应恢复滚动跟随");
    });
    // 清理 new_chat 落盘的会话（~/.mcode/projects/<hash>）
    if let Some(hash_dir) = mcode_core::util::sessions_dir(&c.tmp).parent() {
        let _ = std::fs::remove_dir_all(hash_dir);
    }
}
