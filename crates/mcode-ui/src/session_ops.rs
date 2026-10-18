//! 指令系统与会话操作 — slash 指令本地解释（不进模型回合）、会话列表/恢复、
//! 设置类操作（模型/语言/模式/PUA 切换并重建 harness）、快照导出。

use mcode_core::config::Language;
use mcode_core::util::mcode_dir;
use mcode_core::{agent::history::HistoryStore, agent::Role, sessions::SessionStore};

use crate::app::{ChatMsg, View};

/// 语言循环顺序（设置视图 / /language 用）
pub const LANGS: [Language; 5] = [
    Language::ZhCn,
    Language::ZhTw,
    Language::EnUs,
    Language::JaJp,
    Language::KoKr,
];

pub fn lang_label(l: Language) -> &'static str {
    match l {
        Language::ZhCn => "简体中文 zh_CN",
        Language::ZhTw => "繁體中文 zh_TW",
        Language::EnUs => "English en_US",
        Language::JaJp => "日本語 ja_JP",
        Language::KoKr => "한국어 ko_KR",
    }
}

pub fn lang_next(l: Language) -> Language {
    let i = LANGS.iter().position(|x| *x == l).unwrap_or(0);
    LANGS[(i + 1) % LANGS.len()]
}

impl App {
    // ---------- 通用小工具 ----------

    /// 推一条 Notice 消息
    pub fn push_notice(&mut self, text: String, error: bool) {
        self.messages.push(ChatMsg::Notice { text: text.into(), error });
    }

    /// 刷新会话列表（当前 cwd）
    pub fn refresh_sessions(&mut self) {
        self.sessions = SessionStore::list(&self.cwd).unwrap_or_default();
    }

    /// 刷新上下文占用缓存（hist 锁空闲时才更新）
    pub fn refresh_ctx(&mut self) {
        if let Ok(h) = self.hist.try_lock() {
            self.ctx_tokens = h.estimate_tokens();
        }
    }

    /// 新建会话：清消息/队列/历史，会话置空（goal 保持）
    pub fn new_chat(&mut self) {
        self.messages.clear();
        self.queue.clear();
        if let Ok(mut h) = self.hist.try_lock() { h.clear(); }
        self.session = None;
        self.running = false;
    }

    /// 权限模式循环（真值在 harness.deps.perms；锁被持有时退化为本地缓存循环）
    pub fn cycle_mode(&mut self) {
        self.mode = match self.harness.as_ref().map(|h| h.deps.perms.try_lock()) {
            Some(Ok(mut p)) => p.cycle_mode(),
            _ => self.mode.next(),
        };
    }

    // ---------- 指令系统（返回 true = 本地已解释，不进模型回合） ----------

    pub fn handle_command(&mut self, text: &str, cx: &mut gpui::Context<Self>) -> bool {
        let (cmd, arg) = text.split_once(' ').unwrap_or((text, ""));
        let cmd = cmd.trim().to_lowercase();
        let arg = arg.trim();
        match cmd.as_str() {
            "/help" => {
                let names: Vec<String> = mcode_core::commands::COMMANDS
                    .iter()
                    .map(|c| c.name.to_string())
                    .collect();
                self.push_notice(format!("可用指令（{}）：\n{}", names.len(), names.join("  ")), false);
            }
            "/status" => {
                self.push_notice(
                    format!(
                        "MCode Desktop v0.2.0 · 模型 {} · cwd {} · 模式 {} · tokens {}",
                        self.cfg.active_model().unwrap_or_default(),
                        self.cwd.display(),
                        self.mode.label(),
                        self.tokens_total
                    ),
                    false,
                );
            }
            "/doctor" => {
                let ready = self.cfg.provider_ready().map(|_| "OK").unwrap_or("异常");
                let key = self
                    .cfg
                    .active_provider()
                    .and_then(|p| p.resolve_api_key())
                    .map(|_| "OK")
                    .unwrap_or("缺失");
                self.push_notice(format!("诊断：provider={ready} · api_key={key} · cwd={}", self.cwd.display()), false);
            }
            "/token" => {
                self.push_notice(format!("累计 token：{}（input+output）", self.tokens_total), false);
            }
            "/model" => {
                if arg.is_empty() {
                    let models = self
                        .cfg
                        .active_provider()
                        .map(|p| p.models.join("  "))
                        .unwrap_or_else(|| "（未配置供应商）".into());
                    self.push_notice(format!("可选模型：{models}（/model <name> 切换）"), false);
                } else if self.set_model(arg) {
                    self.push_notice(format!("✓ 模型 → {arg}"), false);
                } else {
                    self.push_notice(format!("✗ 模型 {arg} 不在当前供应商列表中"), true);
                }
            }
            "/language" => {
                if arg.is_empty() {
                    self.push_notice(
                        format!("当前语言：{}（/language <zh_CN|zh_TW|en_US|ja_JP|ko_KR>）", lang_label(self.cfg.language)),
                        false,
                    );
                } else if let Some(l) = parse_language(arg) {
                    self.cfg.language = l;
                    self.save_cfg_and_rebuild(cx);
                    self.push_notice(format!("✓ 语言 → {}", lang_label(l)), false);
                } else {
                    self.push_notice(format!("✗ 无法识别语言 {arg}"), true);
                }
            }
            "/clear" => {
                self.messages.clear();
                if let Ok(mut h) = self.hist.try_lock() { h.clear(); }
                self.session = None;
            }
            "/compact" => {
                if self.hist.try_lock().is_err() {
                    self.push_notice("回合运行中，稍后再压缩".to_string(), true);
                    return true;
                }
                let Ok(mut h) = self.hist.try_lock() else { return true; };
                let msgs = h.message_count();
                let keep = (msgs / 4).max(2);
                let first = h
                    .all()
                    .first()
                    .map(|m| crate::app::trunc(&m.text, 200))
                    .unwrap_or_default();
                let summary = format!("手动压缩：{msgs} 条消息，首条：\"{first}\"");
                h.compact(summary, keep);
                drop(h);
                self.push_notice(format!("🗜 已手动压缩历史（保留最近 {keep} 轮）"), false);
            }
            "/goal" => {
                if arg.is_empty() || arg == "clear" {
                    self.goal = None;
                    self.goal_rounds = 0;
                    self.push_notice("goal 已清除".to_string(), false);
                } else {
                    self.goal = Some(arg.to_string());
                    self.goal_rounds = 0;
                    self.push_notice(format!("🎯 goal 已设置：{arg}（回合完成后自动推进，上限 {} 轮）", mcode_core::agent::context::MAX_GOAL_ROUNDS), false);
                }
            }
            "/agents" | "/skills" | "/mcp" | "/hooks" => {
                self.view = View::Extensions;
            }
            "/memory" => {
                let paths = mcode_core::util::mcode_md_paths(&self.cwd);
                let list = paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n");
                self.push_notice(format!("记忆文件（nearest-first）：\n{list}"), false);
            }
            "/init" => {
                let target = self.cwd.join("MCODE.md");
                if target.exists() {
                    self.push_notice("MCODE.md 已存在，跳过生成".to_string(), false);
                } else {
                    let tpl = "# MCODE\n\n## 项目说明\n- 在此描述项目结构与约定（模型每回合都会读取）\n\n## 常用命令\n- build: \n- test: \n";
                    match std::fs::write(&target, tpl) {
                        Ok(_) => self.push_notice(format!("✓ 已生成 {}", target.display()), false),
                        Err(e) => self.push_notice(format!("✗ 生成失败：{e}"), true),
                    }
                }
            }
            "/save" => {
                self.save_snapshot();
            }
            "/resume" => {
                self.refresh_sessions();
                self.view = View::Sessions;
            }
            "/pua:on" => {
                self.cfg.pua_always_on = true;
                self.save_cfg_and_rebuild(cx);
                self.push_notice("✓ PUA 收敛纪律已开启（每会话常驻）".to_string(), false);
            }
            "/pua:off" => {
                self.cfg.pua_always_on = false;
                self.save_cfg_and_rebuild(cx);
                self.push_notice("✓ PUA 收敛纪律已关闭".to_string(), false);
            }
            "/config" => {
                self.view = View::Settings;
            }
            "/permissions" => {
                let desktop = mcode_core::config::DesktopConfig::load();
                if desktop.permission_rules.is_empty() {
                    self.push_notice("权限规则为空（~/.mcode/desktop.json permissionRules）".to_string(), false);
                } else {
                    let list = desktop
                        .permission_rules
                        .iter()
                        .map(|r| format!("{}({}) → {}", r.tool, r.pattern, if r.allow { "ALLOW" } else { "DENY" }))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.push_notice(format!("权限规则（desktop.json）：\n{list}"), false);
                }
            }
            "/exit" => {
                cx.quit();
            }
            _ => {
                self.push_notice(format!("暂未支持：{cmd}（/help 查看指令列表）"), false);
            }
        }
        true
    }

    // ---------- 配置操作（设置视图复用） ----------

    /// 切换 active_model（校验在当前供应商模型列表内）
    pub fn set_model(&mut self, model: &str) -> bool {
        let valid = self
            .cfg
            .active_provider()
            .map(|p| p.models.iter().any(|m| m == model))
            .unwrap_or(false);
        if !valid {
            return false;
        }
        self.cfg.active_model = Some(model.to_string());
        self.cfg.save().ok();
        true
    }

    /// 配置保存 + harness 重建（旧 harness drop 后确认泵随 ui_tx 关闭退出）
    pub fn save_cfg_and_rebuild(&mut self, cx: &mut gpui::Context<Self>) {
        self.cfg.save().ok();
        self.rebuild_harness(cx);
    }

    /// PUA 开关（设置视图）
    pub fn set_pua(&mut self, on: bool, cx: &mut gpui::Context<Self>) {
        self.cfg.pua_always_on = on;
        self.save_cfg_and_rebuild(cx);
    }

    /// 默认权限模式（设置视图：写配置，重建 harness 生效）
    pub fn set_default_mode(&mut self, mode: mcode_core::permission::Mode, cx: &mut gpui::Context<Self>) {
        self.cfg.default_mode = mode;
        self.save_cfg_and_rebuild(cx);
    }

    // ---------- 会话恢复 ----------

    /// 恢复历史会话：JSONL 富消息 → ChatMsg 映射 + hist 重建（相邻 user/assistant 文本对）
    pub fn resume_session(&mut self, id: &str, cx: &mut gpui::Context<Self>) {
        let (store, msgs) = match SessionStore::load(&self.cwd, id) {
            Ok(r) => r,
            Err(e) => {
                self.push_notice(format!("恢复会话失败：{e}"), true);
                return;
            }
        };
        let mut chat = vec![];
        let mut hist = HistoryStore::default();
        let mut pending_user: Option<String> = None;
        for m in msgs {
            let mut texts: Vec<String> = vec![];
            let mut tool_uses: Vec<(String, String)> = vec![];
            let mut tool_results: Vec<(String, bool)> = vec![];
            for c in &m.content {
                match c {
                    mcode_core::agent::ContentBlock::Text { text } => texts.push(text.clone()),
                    mcode_core::agent::ContentBlock::ToolUse { name, input, .. } => {
                        tool_uses.push((name.clone(), input.to_string()))
                    }
                    mcode_core::agent::ContentBlock::ToolResult { output, success, .. } => {
                        tool_results.push((output.clone(), *success))
                    }
                    _ => {}
                }
            }
            for text in texts {
                match m.role {
                    Role::User => {
                        chat.push(ChatMsg::User { text: text.clone().into(), refs: vec![] });
                        pending_user = Some(text);
                    }
                    Role::Assistant => {
                        chat.push(ChatMsg::Assistant { text: text.clone().into() });
                        if let Some(u) = pending_user.take() {
                            hist.push_turn(u, text);
                        }
                    }
                    Role::Tool => {}
                }
            }
            for (name, input) in tool_uses {
                chat.push(ChatMsg::ToolCard {
                    name: name.into(),
                    args: crate::app::trunc(&input, 200).into(),
                    summary: "".into(),
                    running: false,
                    success: true,
                });
            }
            for (output, success) in tool_results {
                chat.push(ChatMsg::Notice {
                    text: crate::app::trunc(&output, 400).into(),
                    error: !success,
                });
            }
        }
        self.messages = chat;
        if let Ok(mut h) = self.hist.try_lock() { *h = hist; }
        self.tokens_total = store.meta.tokens_total;
        self.session = Some(store);
        self.refresh_ctx();
        self.view = View::Chat;
        cx.notify();
    }

    // ---------- /save 快照 ----------


    // ---------- 补全/上下文/输入历史（字段私有，供渲染层访问） ----------

    /// 上次缓存的上下文 token 估算（渲染用）
    pub fn ctx_tokens_display(&self) -> usize {
        self.ctx_tokens
    }

    pub(crate) fn history_recall_up(&mut self, current: &str) -> Option<String> {
        self.history.recall_up(current)
    }

    pub(crate) fn history_recall_down(&mut self) -> Option<String> {
        self.history.recall_down()
    }

    /// Onboarding 表单保存：构造 Config → save → build_harness → attach_pump → 进入 Chat
    pub fn onboarding_save(&mut self, cx: &mut gpui::Context<Self>) {
        let name = self.ob_name.read(cx).text().trim().to_string();
        let url = self.ob_url.read(cx).text().trim().to_string();
        let key = self.ob_key.read(cx).text().trim().to_string();
        let models_raw = self.ob_models.read(cx).text();
        let models: Vec<String> = models_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if name.is_empty() || url.is_empty() || models.is_empty() {
            self.push_notice("✗ 名称 / baseURL / 模型列表不能为空".to_string(), true);
            cx.notify();
            return;
        }
        let provider = mcode_core::config::Provider {
            name: name.clone(),
            base_url: url,
            api_key_env: key,
            models: models.clone(),
            multimodal_models: vec![],
        };
        self.cfg.providers = vec![provider];
        self.cfg.active_provider = Some(name);
        self.cfg.active_model = Some(models[0].clone());
        self.cfg.save().ok();
        self.rebuild_harness(cx);
        if self.harness.is_some() {
            self.view = View::Chat;
            self.push_notice(format!("✓ 供应商已配置：{}（模型 {}）", self.cfg.active_provider.clone().unwrap_or_default(), models[0]), false);
        }
        cx.notify();
    }

    /// 把当前消息流渲染成 markdown 快照写 ~/.mcode/session-data/<date>-<id>.md
    pub fn save_snapshot(&mut self) {
        let dir = mcode_dir().join("session-data");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.push_notice(format!("✗ 快照目录创建失败：{e}"), true);
            return;
        }
        let id = self
            .session
            .as_ref()
            .map(|s| s.meta.id.clone())
            .unwrap_or_else(|| "adhoc".into());
        let date = chrono_now_date();
        let path = dir.join(format!("{date}-{id}.md"));
        let mut md = String::new();
        for m in &self.messages {
            match m {
                ChatMsg::User { text, .. } => md.push_str(&format!("## 用户\n\n{text}\n\n")),
                ChatMsg::Assistant { text } => md.push_str(&format!("## MCode\n\n{text}\n\n")),
                ChatMsg::Thinking { text } => md.push_str(&format!("> 💭 {}\n\n", crate::app::trunc(text, 200))),
                ChatMsg::ToolCard { name, args, summary, .. } => {
                    md.push_str(&format!("- 🔧 {name} `{args}` {summary}\n"))
                }
                ChatMsg::Diff { path, .. } => md.push_str(&format!("- ✏️ diff {path}\n")),
                ChatMsg::Notice { text, error } => {
                    md.push_str(&format!("- {} {}\n", if *error { "⚠" } else { "ℹ" }, text))
                }
                ChatMsg::Running => {}
            }
        }
        match std::fs::write(&path, md) {
            Ok(_) => self.push_notice(format!("✓ 会话快照已写入 {}", path.display()), false),
            Err(e) => self.push_notice(format!("✗ 快照写入失败：{e}"), true),
        }
    }
}

fn parse_language(s: &str) -> Option<Language> {
    match s.to_lowercase().as_str() {
        "zh_cn" => Some(Language::ZhCn),
        "zh_tw" => Some(Language::ZhTw),
        "en_us" => Some(Language::EnUs),
        "ja_jp" => Some(Language::JaJp),
        "ko_kr" => Some(Language::KoKr),
        _ => None,
    }
}

fn chrono_now_date() -> String {
    // chrono 已在内核依赖树内，但 mcode-ui 未直接依赖；用 std 时间推日期（UTC）
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}{m:02}{d:02}")
}

/// 天数 → (年, 月, 日)（Howard Hinnant 的 civil_from_days 算法）
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

use crate::app::App;
