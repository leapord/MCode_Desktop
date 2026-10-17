//! 工具体系 — 对齐 MCode.md 3.1：工作区路径收敛（防逃逸）+ diff 产出 + 注册表。

pub mod builtin;
pub mod types;

pub use types::*;

use anyhow::Result;
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

/// 统一 diff 行级数据（edit_file / write_file 产出，UI 直接渲染）
pub fn unified_diff(old: &str, new: &str, path: &str) -> Vec<DiffLine> {
    use similar::ChangeTag;
    let diff = similar::TextDiff::from_lines(old, new);
    let mut out = vec![];
    out.push(DiffLine { kind: DiffKind::Hunk, text: format!("@@ {path} @@") });
    for change in diff.iter_all_changes() {
        let line = match change.tag() {
            ChangeTag::Equal => DiffLine { kind: DiffKind::Context, text: change.to_string() },
            ChangeTag::Insert => DiffLine { kind: DiffKind::Add, text: change.to_string() },
            ChangeTag::Delete => DiffLine { kind: DiffKind::Delete, text: change.to_string() },
        };
        out.push(line);
    }
    out
}

pub fn diff_stats(lines: &[DiffLine]) -> (usize, usize) {
    let add = lines.iter().filter(|l| l.kind == DiffKind::Add).count();
    let del = lines.iter().filter(|l| l.kind == DiffKind::Delete).count();
    (add, del)
}

/// 工作区路径收敛（防路径逃逸，对齐 CLI validate.ts）
pub fn contain_to_workspace(cwd: &Path, rel: &str) -> Result<PathBuf> {
    let root = cwd.canonicalize()?;
    let joined = root.join(rel);
    if let Ok(c) = joined.canonicalize() {
        if c.starts_with(&root) {
            return Ok(c);
        }
        anyhow::bail!("路径逃逸被拒绝: {rel}");
    }
    // 目标不存在（write 新文件）：逐段校验，拒绝 `..` 与绝对路径组件
    let suffix = joined.strip_prefix(&root)?;
    let mut norm = root.clone();
    for comp in suffix.components() {
        match comp {
            Component::Normal(seg) => norm.push(seg),
            Component::CurDir => {}
            _ => anyhow::bail!("路径逃逸被拒绝: {rel}"),
        }
    }
    Ok(norm)
}

/// 工具执行上下文
#[derive(Clone)]
pub struct ToolContext {
    pub cwd: PathBuf,
    pub cancel: tokio::sync::watch::Receiver<bool>,
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn def(&self) -> &'static ToolDefinition;
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult>;
}

/// 工具注册表 — 收纳 + 按名检索（对齐 tools/registry.ts）
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn with_builtins() -> Self {
        let mut r = Self { tools: vec![] };
        r.tools.push(Box::new(builtin::ReadFile));
        r.tools.push(Box::new(builtin::WriteFile));
        r.tools.push(Box::new(builtin::EditFile));
        r.tools.push(Box::new(builtin::Bash));
        r.tools.push(Box::new(builtin::Glob));
        r.tools.push(Box::new(builtin::Grep));
        r
    }

    pub fn register(&mut self, t: Box<dyn Tool>) {
        self.tools.push(t);
    }

    pub fn definitions(&self) -> Vec<&'static ToolDefinition> {
        self.tools.iter().map(|t| t.def()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&(dyn Tool + '_)> {
        self.tools.iter().map(|t| t.as_ref()).find(|t| t.def().name == name)
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.tools.iter().map(|t| t.def().name).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_escape() {
        let cwd = std::env::temp_dir().join("mcore-test-cwd");
        std::fs::create_dir_all(&cwd).unwrap();
        assert!(contain_to_workspace(&cwd, "../../etc/passwd").is_err());
        assert!(contain_to_workspace(&cwd, "src/a.rs").is_ok());
    }

    #[test]
    fn diff_marks_add_delete() {
        let lines = unified_diff("a\nb\n", "a\nc\n", "f.rs");
        let (a, d) = diff_stats(&lines);
        assert_eq!((a, d), (1, 1));
    }

    #[test]
    fn registry_has_six_builtins() {
        let r = ToolRegistry::with_builtins();
        assert_eq!(r.names(), vec!["read_file", "write_file", "edit_file", "bash", "glob", "grep"]);
    }
}
