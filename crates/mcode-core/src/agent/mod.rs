//! Agent harness — compact-harness 模式（对齐 MCode.md 1.2/4.3）：
//! context（系统提示词组装）/ history（消息管理）/ compaction（阈值压缩）/ 主循环编排。

pub mod completion;

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ---------- 消息模型 ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Image { media_type: String, data: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { call_id: String, output: String, success: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub ts: chrono::DateTime<chrono::Utc>,
}

impl Message {
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self { role, content: vec![ContentBlock::Text { text: text.into() }], ts: chrono::Utc::now() }
    }

    /// 多模态用户消息：图片附件仅当模型带 multimodal 标记时进入上下文，
    /// 否则拒绝并降级为文本提示（对齐 CLI 近期修复的语义）
    pub fn user_with_images(
        text: impl Into<String>,
        images: Vec<(String, String)>, // (name, data_url)
        model_multimodal: bool,
    ) -> Self {
        let mut content = vec![ContentBlock::Text { text: text.into() }];
        for (name, data) in images {
            if model_multimodal {
                content.push(ContentBlock::Image { media_type: "image/png".into(), data });
            } else {
                content.push(ContentBlock::Text {
                    text: format!("[图片已忽略：{name}，当前模型不支持多模态]"),
                });
            }
        }
        Self { role: Role::User, content, ts: chrono::Utc::now() }
    }
}

/// 提炼纯文本（渲染 / 持久化摘要用）
pub fn text_of(m: &Message) -> String {
    m.content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------- harness ----------

/// 上下文预算（自动 compact 阈值）
pub const CONTEXT_LIMIT_TOKENS: usize = 160_000;
pub const AUTO_COMPACT_THRESHOLD: f64 = 0.62;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnResult {
    pub reply: String,
    pub tool_calls: usize,
    pub interrupted: bool,
}

/// 模型回调抽象：api 层实现它，loop 只做编排（可测试）
#[async_trait::async_trait]
pub trait ModelBackend: Send + Sync {
    /// 一轮完整 agent 循环：流式产出文本与工具调用事件
    async fn run_turn(
        &self,
        system: &str,
        history: &[Message],
        tools: &[crate::tools::ToolDefinition],
        on_event: &mut (dyn FnMut(TurnEvent) + Send),
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<TurnResult>;
}

#[derive(Debug, Clone)]
pub enum TurnEvent {
    Thinking(String),
    TextDelta(String),
    ToolCallStart { id: String, name: String, input: serde_json::Value },
    ToolCallEnd { id: String, output: String, success: bool },
    Usage(crate::util::TokenUsage),
}

/// 系统提示词组装（对齐 harness/context.ts）：身份 + 工具说明 + skill 索引 + 环境信息 + 语言指令
pub fn build_system_prompt(cfg: &crate::config::Config, skills: &[crate::skills::SkillEntry], cwd: &str) -> String {
    let lang = match cfg.language {
        crate::config::Language::ZhCn => "Always respond in Simplified Chinese.",
        crate::config::Language::ZhTw => "Always respond in Traditional Chinese.",
        crate::config::Language::EnUs => "Always respond in English.",
        crate::config::Language::JaJp => "Always respond in Japanese.",
        crate::config::Language::KoKr => "Always respond in Korean.",
    };
    let skill_index: String = skills
        .iter()
        .map(|s| format!("- {}: {}", s.name, s.description))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "你是 MCode，一个桌面端 AI 编程智能体。\n{lang}\n当前工作目录: {cwd}\n\n可用技能索引（渐进式披露，仅 name+description 常驻）:\n{skill_index}"
    )
}

/// 上下文占用估算（4 chars ≈ 1 token 的粗估，UI 展示用）
pub fn estimate_tokens(history: &[Message], system: &str) -> usize {
    let chars: usize = history.iter().map(|m| text_of(m).len()).sum();
    chars / 4 + system.len() / 4
}

/// 是否需要自动 compact
pub fn needs_compaction(history: &[Message], system: &str) -> bool {
    let t = estimate_tokens(history, system);
    t as f64 > CONTEXT_LIMIT_TOKENS as f64 * AUTO_COMPACT_THRESHOLD
}

/// 压缩：保留最近 keep 轮，早期消息摘要化（对齐 harness/compaction.ts 语义）
pub fn compact(history: Vec<Message>, keep: usize) -> (Message, Vec<Message>) {
    if history.len() <= keep {
        return (Message::text(Role::User, "[无早期历史]"), history);
    }
    let (early, recent) = history.split_at(history.len() - keep);
    let summary: String = early
        .iter()
        .map(|m| format!("[{:?}] {}", m.role, text_of(m)))
        .collect::<Vec<_>>()
        .join("\n");
    let summary_msg = Message::text(
        Role::User,
        format!("[早期历史摘要 — /compact 生成]\n{}", truncate(&summary, 4000)),
    );
    (summary_msg, recent.to_vec())
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multimodal_gate_drops_images_without_flag() {
        let with = Message::user_with_images("看图", vec![("a.png".into(), "data:...".into())], true);
        assert!(matches!(with.content[1], ContentBlock::Image { .. }));

        let without = Message::user_with_images("看图", vec![("a.png".into(), "data:...".into())], false);
        assert!(text_of(&without).contains("图片已忽略"));
    }

    #[test]
    fn compaction_keeps_recent() {
        let history: Vec<Message> = (0..10).map(|i| Message::text(Role::User, format!("m{i}"))).collect();
        let (summary, recent) = compact(history, 4);
        assert_eq!(recent.len(), 4);
        assert!(crate::agent::text_of(&summary).contains("m0"));
        assert!(!crate::agent::text_of(&recent[0]).contains("m0"));
    }
}
