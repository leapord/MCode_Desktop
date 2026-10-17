//! 工具类型定义 — 对齐 MCode.md 3.1 Tool 接口

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value,
    pub is_read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    /// 回传给模型的文本
    pub output: String,
    /// 给 UI 的渲染块（diff / 输出卡片）
    pub display: Option<DisplayBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DisplayBlock {
    Diff { path: String, lines: Vec<DiffLine> },
    Output { stdout: String, stderr: String, code: i32 },
    FileRef { path: String, lines: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffKind {
    Context,
    Add,
    Delete,
    Hunk,
}
