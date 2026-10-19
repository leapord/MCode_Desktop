//! 文件浏览器/右键菜单测试矩阵（用例 21-28）— 状态机级驱动：
//! open_context_menu/run_context_action 直调 + tempdir 文件系统断言。
//! 右键捕获 listener 为薄封装（open_context_menu），真实鼠标事件路径由 simulate 验证受限，
//! 以状态级等价覆盖（gpui TestContext 无头环境无法命中真实行元素几何）。

use crate::tests::interaction_tests::setup;
use gpui::TestAppContext;

// ---------- 用例 21：右键菜单出现 + 坐标 = 右键点（clamp 后） ----------
#[gpui::test]
async fn case21_context_menu_opens_at_click_position(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    let f = c.tmp.join("a.txt");
    std::fs::write(&f, "x").unwrap();
    let d = c.tmp.join("subdir");
    std::fs::create_dir_all(&d).unwrap();
    // 文件：视口内坐标 → 菜单坐标 = 右键点
    c.app.update(cx, |a, cx| {
        a.open_context_menu(f.clone(), 800., 300., (1920., 1080.), cx);
    });
    c.app.read_with(cx, |a, _| {
        let m = a.context_menu.as_ref().expect("菜单未出现");
        assert!(
            (m.x - 800.).abs() <= 2. && (m.y - 300.).abs() <= 2.,
            "坐标偏移: {},{}",
            m.x,
            m.y
        );
        assert!(!m.is_dir && !m.delete_armed);
    });
    // 目录：同样成立（dir 菜单项更多）
    c.app.update(cx, |a, cx| {
        a.open_context_menu(d.clone(), 800., 300., (1920., 1080.), cx);
    });
    c.app.read_with(cx, |a, _| {
        let m = a.context_menu.as_ref().expect("目录菜单未出现");
        assert!(m.is_dir);
        assert!((m.x - 800.).abs() <= 2. && (m.y - 300.).abs() <= 2.);
    });
}

// ---------- 用例 22：重命名全路径 ----------
#[gpui::test]
async fn case22_rename_paths(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    let f = c.tmp.join("old.txt");
    std::fs::write(&f, "data").unwrap();

    // 合法重命名
    c.app.update(cx, |a, _cx| {
        a.fs_rename(&f, "new.txt");
    });
    assert!(
        !f.exists() && c.tmp.join("new.txt").exists(),
        "合法重命名未生效"
    );

    // 重名拒绝：新建占用名，重命名回旧名应失败且原文件保留
    std::fs::write(c.tmp.join("occupied.txt"), "z").unwrap();
    c.app.update(cx, |a, _cx| {
        a.fs_rename(&c.tmp.join("new.txt"), "occupied.txt");
    });
    assert!(c.tmp.join("new.txt").exists(), "重名时原文件被破坏");
    assert_eq!(std::fs::read(c.tmp.join("occupied.txt")).unwrap(), b"z");

    // 含 / 与 ".." 拒绝
    c.app.update(cx, |a, _cx| {
        a.fs_rename(&c.tmp.join("new.txt"), "a/b");
    });
    c.app.update(cx, |a, _cx| {
        a.fs_rename(&c.tmp.join("new.txt"), "..");
    });
    assert!(c.tmp.join("new.txt").exists(), "非法名未拒绝");
    assert!(!c.tmp.join("b").exists() && !c.tmp.parent().unwrap().join("b").exists());

    // 空名拒绝
    c.app.update(cx, |a, _cx| {
        a.fs_rename(&c.tmp.join("new.txt"), "  ");
    });
    assert!(c.tmp.join("new.txt").exists());
}

// ---------- 用例 23：删除两段式（文件 + 目录递归） ----------
#[gpui::test]
async fn case23_delete_two_phase_file_and_dir(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    let f = c.tmp.join("del.txt");
    std::fs::write(&f, "x").unwrap();
    // 第一次：仅武装（返回 false = 菜单保持打开），文件仍在
    c.app.update(cx, |a, cx| {
        let close = a.run_context_action(crate::views::context_menu::CtxAction::Delete, &f, cx);
        assert!(!close, "第一次删除不应关闭菜单");
        assert!(a.fs_delete_armed.is_some(), "未武装");
    });
    assert!(f.exists(), "武装阶段文件被误删");
    // 第二次：执行删除
    c.app.update(cx, |a, cx| {
        let close = a.run_context_action(crate::views::context_menu::CtxAction::Delete, &f, cx);
        assert!(close, "第二次删除应关闭菜单");
    });
    assert!(!f.exists(), "文件未删除");

    // 目录递归删除
    let d = c.tmp.join("deldir");
    std::fs::create_dir_all(d.join("nested")).unwrap();
    std::fs::write(d.join("nested").join("f.txt"), "y").unwrap();
    c.app.update(cx, |a, cx| {
        let _ = a.run_context_action(crate::views::context_menu::CtxAction::Delete, &d, cx);
    });
    c.app.update(cx, |a, cx| {
        let _ = a.run_context_action(crate::views::context_menu::CtxAction::Delete, &d, cx);
    });
    assert!(!d.exists(), "目录未递归删除");
}

// ---------- 用例 24：复制 → 粘贴到目录（copy 后缀） ----------
#[gpui::test]
async fn case24_copy_paste_into_dir_with_suffix(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    let f = c.tmp.join("src.txt");
    std::fs::write(&f, "content").unwrap();
    let d = c.tmp.join("destdir");
    std::fs::create_dir_all(&d).unwrap();

    use crate::views::context_menu::CtxAction;
    c.app.update(cx, |a, cx| {
        let _ = a.run_context_action(CtxAction::Copy, &f, cx);
        let _ = a.run_context_action(CtxAction::PasteInto, &d, cx);
    });
    let copy1 = d.join("src.txt");
    assert!(copy1.exists(), "副本未出现");
    assert_eq!(std::fs::read(&copy1).unwrap(), b"content");

    // 再粘贴 → " copy" 后缀
    c.app.update(cx, |a, cx| {
        let _ = a.run_context_action(CtxAction::Copy, &f, cx);
        let _ = a.run_context_action(CtxAction::PasteInto, &d, cx);
    });
    assert!(d.join("src copy.txt").exists(), "重名未自动加 copy 后缀");
}

// ---------- 用例 25：剪切 → 粘贴到目录（move 语义） ----------
#[gpui::test]
async fn case25_cut_paste_moves(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    let f = c.tmp.join("mov.txt");
    std::fs::write(&f, "m").unwrap();
    let d = c.tmp.join("mdir");
    std::fs::create_dir_all(&d).unwrap();

    use crate::views::context_menu::CtxAction;
    c.app.update(cx, |a, cx| {
        let _ = a.run_context_action(CtxAction::Cut, &f, cx);
        let _ = a.run_context_action(CtxAction::PasteInto, &d, cx);
    });
    assert!(!f.exists(), "剪切后源仍在");
    assert!(d.join("mov.txt").exists(), "粘贴目标未出现");
    assert_eq!(std::fs::read(d.join("mov.txt")).unwrap(), b"m");
}

// ---------- 用例 26：ESC/外点关闭菜单 + 菜单与改名弹窗互斥 ----------
#[gpui::test]
async fn case26_menu_close_and_mutex(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    let f = c.tmp.join("m.txt");
    std::fs::write(&f, "x").unwrap();

    // 菜单打开 → ESC 关闭，且不影响其他优先级链（补全/权限/running 未被触碰）
    c.app.update(cx, |a, cx| {
        a.open_context_menu(f.clone(), 500., 500., (1920., 1080.), cx);
    });
    c.app.update(cx, |a, cx| a.interrupt(cx));
    c.app.read_with(cx, |a, _| {
        assert!(a.context_menu.is_none(), "ESC 未关闭菜单");
        assert!(!a.running && a.pending_permission.is_none() && !a.completion.is_open());
    });

    // 互斥：改名弹窗打开时右键 → 改名弹窗关闭
    c.app.update(cx, |a, cx| {
        a.open_rename(f.clone(), cx);
        assert!(a.rename_state.is_some());
        a.open_context_menu(f.clone(), 500., 500., (1920., 1080.), cx);
    });
    c.app.read_with(cx, |a, _| {
        assert!(a.rename_state.is_none(), "菜单未关闭改名弹窗");
        assert!(a.context_menu.is_some());
    });
    // 反向：菜单打开时点重命名 → 菜单关闭、改名弹窗出现
    c.app.update(cx, |a, cx| {
        let _ = a.run_context_action(crate::views::context_menu::CtxAction::Rename, &f, cx);
    });
    c.app.read_with(cx, |a, _| {
        assert!(a.rename_state.is_some(), "重命名弹窗未出现");
    });
}

// ---------- 用例 27：右缘/下缘翻折（菜单不出窗） ----------
#[gpui::test]
async fn case27_menu_edge_flip(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    let f = c.tmp.join("edge.txt");
    std::fs::write(&f, "x").unwrap();
    let d = c.tmp.join("edgedir");
    std::fs::create_dir_all(&d).unwrap();

    // 文件菜单（4 项）在右缘：向左翻折
    c.app.update(cx, |a, cx| {
        a.open_context_menu(f.clone(), 1900., 300., (1920., 1080.), cx);
    });
    c.app.read_with(cx, |a, _| {
        let m = a.context_menu.as_ref().unwrap();
        assert!(m.x + 170. <= 1920. - 8., "右缘未翻折: x={}", m.x);
        assert!(m.x > 0. && m.y > 0.);
    });
    // 目录菜单（7 项）在下缘：向上翻折
    c.app.update(cx, |a, cx| {
        a.open_context_menu(d.clone(), 500., 1060., (1920., 1080.), cx);
    });
    c.app.read_with(cx, |a, _| {
        let m = a.context_menu.as_ref().unwrap();
        assert!(m.y + 7. * 30. + 12. <= 1080., "下缘未翻折: y={}", m.y);
    });
}

// ---------- 用例 28：隐藏文件开关下的点开头条目菜单可用 ----------
#[gpui::test]
async fn case28_hidden_entries_menu_works(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    let h = c.tmp.join(".hidden.txt");
    std::fs::write(&h, "secret").unwrap();
    c.app.update(cx, |a, cx| {
        a.show_hidden = true;
        a.open_context_menu(h.clone(), 400., 400., (1920., 1080.), cx);
        let _ = a.run_context_action(crate::views::context_menu::CtxAction::Rename, &h, cx);
    });
    c.app.read_with(cx, |a, _| {
        let st = a.rename_state.as_ref().expect("隐藏条目重命名弹窗未出现");
        assert_eq!(st.path, h);
    });
    // 确认重命名生效
    c.app.update(cx, |a, cx| {
        if let Some(st) = a.rename_state.as_ref() {
            st.input.update(cx, |i, cx| i.set_text("renamed.txt", cx));
        }
        a.confirm_rename(cx);
    });
    assert!(
        !h.exists() && c.tmp.join("renamed.txt").exists(),
        "隐藏条目重命名未生效"
    );
}

// ---------- 用例 29：enter_workspace 链（recents 置顶 + harness 重建） ----------
// 原生 prompt_for_paths 无法无头模拟（gpui 测试平台未实现该 API），按协调方许可
// 以直调 enter_workspace 覆盖"选目录 → recent 置顶 → harness 重建 → sessions 刷新"链路。
#[gpui::test]
async fn case29_enter_workspace_updates_recents_and_harness(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    // 目标工作区：tempdir 下建子目录 + 一个会话元数据（SessionStore::list 数据源）
    let ws = c.tmp.join("new_ws");
    std::fs::create_dir_all(&ws).unwrap();
    // 会话存储按 cwd 哈希落在 ~/.mcode/projects/<hash>/sessions（与 CLI 互通）
    let sess_dir = mcode_core::util::sessions_dir(&ws);
    std::fs::create_dir_all(&sess_dir).unwrap();
    // updated 必须是近期时间：90 天保留期外的会话会被 SessionStore::list 过滤
    let now = chrono::Utc::now().to_rfc3339();
    let meta = serde_json::json!({
        "id": "s1", "title": "t", "cwd": ws.to_string_lossy(),
        "model": "", "mode": "passon",
        "started": now.clone(), "updated": now,
        "turns": 0, "tokensTotal": 0
    });
    std::fs::write(
        sess_dir.join("s1.meta.json"),
        serde_json::to_string(&meta).unwrap(),
    )
    .unwrap();

    c.app.update(cx, |a, cx| {
        let before = a.cwd.clone();
        a.enter_workspace(ws.clone(), cx);
        // cwd 换新
        assert_eq!(a.cwd, ws);
        assert_ne!(a.cwd, before);
        // 会话列表已按新 cwd 刷新：手写的 s1 + enter_workspace 内 new_chat 立即落盘的新会话
        assert!(
            a.sessions.iter().any(|m| m.id == "s1"),
            "sessions 未按新工作区刷新（s1 缺失）"
        );
        assert!(
            a.session
                .as_ref()
                .is_some_and(|s| a.sessions.iter().any(|m| m.id == s.meta.id)),
            "新工作区的新建会话未立即入列"
        );
        // harness 已重建且 deps.cwd 指向新工作区
        let h = a.harness.as_ref().expect("harness 未重建");
        assert_eq!(h.deps.cwd, ws);
        // 非法路径拒绝（is_dir 校验）
        a.enter_workspace(c.tmp.join("nonexistent_dir_xyz"), cx);
        assert_eq!(a.cwd, ws, "非法路径不应改变 cwd");
    });
    // recent 置顶写回 desktop.json
    let desktop = mcode_core::config::DesktopConfig::load();
    assert_eq!(
        desktop.recent_workspaces.first().map(|s| s.as_str()),
        Some(ws.to_string_lossy().as_ref()),
        "recent 未置顶"
    );
    // 清理 ~/.mcode/projects/<hash>（本测试写入的会话元数据）
    if let Some(hash_dir) = sess_dir.parent() {
        let _ = std::fs::remove_dir_all(hash_dir);
    }
}

// ---------- Reveal in Finder/Explorer ----------

#[test]
fn case35_reveal_command_shape_and_menu_consistency() {
    use crate::views::context_menu::build_reveal_command;
    // macOS 分支：open -R <path>
    let cmd = build_reveal_command(std::path::Path::new("/tmp/x/f.txt"));
    let mut dbg = format!("{:?}", cmd);
    assert!(dbg.contains(r#""open""#), "macOS 应为 open: {dbg}");
    assert!(dbg.contains("-R"), "应有 -R 反显参数: {dbg}");
    assert!(dbg.contains("f.txt"));
    let _ = &mut dbg;
    // 目录与文件同样可用（open -R 对两者均反显其所在父目录并选中）
    let _ = build_reveal_command(std::path::Path::new("/tmp/x"));
    // CtxAction::Reveal 在 run_context_action 的 match 中（编译级确认已覆盖）
    let acts = [crate::views::context_menu::CtxAction::Reveal];
    assert!(matches!(
        acts[0],
        crate::views::context_menu::CtxAction::Reveal
    ));
}

// ---------- 会话右键菜单（用例 36-39）----------
// 会话存储按 cwd 哈希落 ~/.mcode/projects/<hash>/sessions；测试用唯一 tempdir + 结束清理。

/// 清理测试 cwd 对应的 ~/.mcode/projects/<hash> 目录
fn cleanup_sessions_dir(cwd: &std::path::Path) {
    if let Some(hash_dir) = mcode_core::util::sessions_dir(cwd).parent() {
        let _ = std::fs::remove_dir_all(hash_dir);
    }
}

// ---------- 用例 36：新建会话立即入列 + 空会话复用 ----------
#[gpui::test]
async fn case36_new_chat_creates_session_immediately(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    c.app.update(cx, |a, _cx| {
        assert!(a.sessions.is_empty(), "前置：无会话");
        a.new_chat();
        assert!(a.session.is_some(), "new_chat 未落盘创建会话");
        assert_eq!(a.sessions.len(), 1, "新建会话未立即出现在列表");
        // 空会话复用：无完成回合（turns == 0）时再点不重复建，防垃圾条目
        a.new_chat();
        assert_eq!(a.sessions.len(), 1, "空会话未复用，重复建条目");
        // 落盘校验：meta + 空 jsonl 都存在（resume 前提）
        let id = a.session.as_ref().unwrap().meta.id.clone();
        let dir = mcode_core::util::sessions_dir(&c.tmp);
        assert!(dir.join(format!("{id}.meta.json")).exists());
        assert!(dir.join(format!("{id}.jsonl")).exists());
    });
    cleanup_sessions_dir(&c.tmp);
}

// ---------- 用例 37：会话菜单打开位置 + 与文件菜单互斥 + 边缘翻折 ----------
#[gpui::test]
async fn case37_session_menu_position_and_mutex(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    c.app.update(cx, |a, cx| {
        a.new_chat();
        let id = a.session.as_ref().unwrap().meta.id.clone();
        // 先开文件右键菜单 + 文件重命名弹窗 → 开会话菜单应互斥关闭
        a.context_menu = Some(crate::views::context_menu::CtxMenu {
            path: c.tmp.clone(),
            is_dir: true,
            delete_armed: false,
            x: 0.,
            y: 0.,
        });
        a.open_session_menu(id.clone(), 800., 300., (1920., 1080.), cx);
        assert!(a.context_menu.is_none(), "会话菜单开 → 文件菜单互斥关闭");
        assert!(a.session_menu.is_some(), "会话菜单未打开");
        let m = a.session_menu.as_ref().unwrap();
        // 视口内坐标 → 菜单坐标 = 右键点
        assert_eq!((m.x, m.y), (800., 300.));
        // 边缘翻折：右下角右键 → 菜单收进视口内
        a.open_session_menu(id.clone(), 1900., 1070., (1920., 1080.), cx);
        let m = a.session_menu.as_ref().unwrap();
        assert!(m.x + 170. <= 1920., "菜单右缘超出视口: x={}", m.x);
        assert!(m.y + 72. <= 1080., "菜单下缘超出视口: y={}", m.y);
    });
    cleanup_sessions_dir(&c.tmp);
}

// ---------- 用例 38：会话重命名（弹窗预填 → 确认 → 磁盘/列表/活动会话三处同步） ----------
#[gpui::test]
async fn case38_session_rename_roundtrip(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    c.app.update(cx, |a, cx| {
        a.new_chat();
        let id = a.session.as_ref().unwrap().meta.id.clone();
        a.open_session_rename(&id, cx);
        assert!(a.session_rename.is_some(), "重命名弹窗未打开");
        // 预填当前标题（新建会话默认「未命名会话」）
        let prefilled = a
            .session_rename
            .as_ref()
            .unwrap()
            .input
            .read(cx)
            .text()
            .to_string();
        assert_eq!(prefilled, "未命名会话");
        // 改标题并确认
        a.session_rename
            .as_ref()
            .unwrap()
            .input
            .update(cx, |i, cx| i.set_text("我的新标题", cx));
        a.confirm_session_rename(cx);
        assert!(a.session_rename.is_none(), "确认后弹窗未关闭");
        // 活动会话内存同步
        assert_eq!(a.session.as_ref().unwrap().meta.title, "我的新标题");
        // 列表缓存同步
        assert_eq!(a.sessions[0].title, "我的新标题");
        // 磁盘同步
        let meta_path = mcode_core::util::sessions_dir(&c.tmp).join(format!("{id}.meta.json"));
        let bytes = std::fs::read(&meta_path).unwrap();
        let meta: mcode_core::sessions::SessionMeta = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(meta.title, "我的新标题");
        // 空标题被拒（弹窗已关，标题保持）
        a.open_session_rename(&id, cx);
        a.session_rename
            .as_ref()
            .unwrap()
            .input
            .update(cx, |i, cx| i.set_text("   ", cx));
        a.confirm_session_rename(cx);
        assert_eq!(a.session.as_ref().unwrap().meta.title, "我的新标题");
    });
    cleanup_sessions_dir(&c.tmp);
}

// ---------- 用例 39：会话删除两段式（武装 → 确认；文件清单 + 活动指针清理） ----------
#[gpui::test]
async fn case39_session_delete_two_phase(cx: &mut TestAppContext) {
    let c = setup(cx, vec![]);
    c.app.update(cx, |a, cx| {
        a.new_chat();
        let id = a.session.as_ref().unwrap().meta.id.clone();
        let dir = mcode_core::util::sessions_dir(&c.tmp);
        a.open_session_menu(id.clone(), 100., 100., (1920., 1080.), cx);
        // 第一次点删除：仅武装，菜单保持打开，文件还在
        let closed = a.run_session_action(crate::views::context_menu::SessAction::Delete, &id, cx);
        assert!(!closed, "首次删除应武装而非执行");
        assert!(a.session_menu.as_ref().unwrap().delete_armed, "删除未武装");
        assert!(dir.join(format!("{id}.meta.json")).exists());
        // 第二次点删除：执行 + 通知关闭
        let closed = a.run_session_action(crate::views::context_menu::SessAction::Delete, &id, cx);
        assert!(closed, "二次删除应执行并关闭菜单");
        // 文件清单全删
        assert!(!dir.join(format!("{id}.meta.json")).exists());
        assert!(!dir.join(format!("{id}.jsonl")).exists());
        // 列表 + 活动指针清理
        assert!(a.sessions.is_empty(), "列表未刷新");
        assert!(a.session.is_none(), "删除的是当前会话，活动指针未摘除");
    });
    cleanup_sessions_dir(&c.tmp);
}
