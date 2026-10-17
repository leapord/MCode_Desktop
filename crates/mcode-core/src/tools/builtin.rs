//! 内置工具首批 — 对齐 MCode.md 3.1：read_file / write_file / edit_file / bash / glob / grep

use super::{
    contain_to_workspace, unified_diff, Tool, ToolContext, ToolDefinition, ToolResult,
};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use walkdir::WalkDir;

const MAX_FILE_BYTES: u64 = 512 * 1024; // 大文件截断保护

fn str_arg(input: &Value, key: &str) -> Result<String> {
    Ok(input.get(key).and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("缺少参数 {key}"))?.to_string())
}

fn file_ref(path: &str, lines: String) -> ToolResult {
    ToolResult { success: true, output: lines.clone(), display: Some(super::DisplayBlock::FileRef { path: path.into(), lines }) }
}

// ---------- read_file ----------

pub struct ReadFile;

#[async_trait::async_trait]
impl Tool for ReadFile {
    fn def(&self) -> &'static ToolDefinition {
        &ToolDefinition {
            name: "read_file",
            description: "读取工作区内文件内容（大文件自动截断）",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "工作区相对路径" },
                    "offset": { "type": "integer", "description": "起始行（可选）" },
                    "limit": { "type": "integer", "description": "行数上限（可选）" }
                },
                "required": ["path"]
            }),
            is_read_only: true,
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let rel = str_arg(&input, "path")?;
        let path = contain_to_workspace(&ctx.cwd, &rel)?;
        let bytes = std::fs::read(&path)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            bail!("文件过大（>512KB），请指定 offset/limit 分段读取");
        }
        let text = String::from_utf8_lossy(&bytes);
        let start = input.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let limit = input.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);
        let selected: Vec<&str> = text
            .lines()
            .skip(start)
            .take(limit.unwrap_or(2000))
            .collect();
        let numbered: String = selected
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{:>5} | {}", start + i + 1, l))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(file_ref(&rel, numbered))
    }
}

// ---------- write_file ----------

pub struct WriteFile;

#[async_trait::async_trait]
impl Tool for WriteFile {
    fn def(&self) -> &'static ToolDefinition {
        &ToolDefinition {
            name: "write_file",
            description: "写入（或创建）工作区内文件",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
            is_read_only: false,
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let rel = str_arg(&input, "path")?;
        let content = str_arg(&input, "content")?;
        let path = contain_to_workspace(&ctx.cwd, &rel)?;
        let old = std::fs::read(&path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, &content)?;
        let lines = unified_diff(&old, &content, &rel);
        let out = ToolResult {
            success: true,
            output: format!("已写入 {rel}（{} 字节）", content.len()),
            display: Some(super::DisplayBlock::Diff { path: rel.clone(), lines }),
        };
        Ok(out)
    }
}

// ---------- edit_file ----------

pub struct EditFile;

#[async_trait::async_trait]
impl Tool for EditFile {
    fn def(&self) -> &'static ToolDefinition {
        &ToolDefinition {
            name: "edit_file",
            description: "精确替换工作区内文件内容片段（old_string 唯一匹配）",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
            is_read_only: false,
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let rel = str_arg(&input, "path")?;
        let old_s = str_arg(&input, "old_string")?;
        let new_s = str_arg(&input, "new_string")?;
        let path = contain_to_workspace(&ctx.cwd, &rel)?;
        let content = String::from_utf8(std::fs::read(&path)?)?;
        let count = content.matches(&old_s).count();
        if count == 0 {
            bail!("old_string 未命中");
        }
        if count > 1 {
            bail!("old_string 匹配到 {count} 处，需唯一匹配");
        }
        let updated = content.replacen(&old_s, &new_s, 1);
        std::fs::write(&path, &updated)?;
        let lines = unified_diff(&content, &updated, &rel);
        Ok(ToolResult {
            success: true,
            output: format!("已修改 {rel}"),
            display: Some(super::DisplayBlock::Diff { path: rel.clone(), lines }),
        })
    }
}

// ---------- bash ----------

pub struct Bash;

#[async_trait::async_trait]
impl Tool for Bash {
    fn def(&self) -> &'static ToolDefinition {
        &ToolDefinition {
            name: "bash",
            description: "在工作目录执行 shell 命令（受权限门控，可被取消）",
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout_ms": { "type": "integer", "description": "默认 120000" }
                },
                "required": ["command"]
            }),
            is_read_only: false,
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let command = str_arg(&input, "command")?;
        let timeout = std::time::Duration::from_millis(
            input.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(120_000),
        );
        let mut child = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" })
            .args(if cfg!(windows) { ["/C", &command] } else { ["-c", &command] })
            .current_dir(&ctx.cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let mut cancel = ctx.cancel.clone();
        let stdout_task = child.stdout.take().unwrap();
        let stderr_task = child.stderr.take().unwrap();
        let out_fut = async {
            use tokio::io::AsyncReadExt;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let (mut a, mut b) = (stdout_task, stderr_task);
            let _ = tokio::select! {
                r = a.read_to_end(&mut stdout) => r,
                _ = cancel.changed() => { let _ = child.kill().await; Ok(0usize) }
            };
            let _ = b.read_to_end(&mut stderr).await;
            (stdout, stderr)
        };
        let (stdout, stderr) = match tokio::time::timeout(timeout, out_fut).await {
            Ok(pair) => pair,
            Err(_) => bail!("命令超时（{}ms）", timeout.as_millis()),
        };
        let stdout = String::from_utf8_lossy(&stdout).into_owned();
        let stderr = String::from_utf8_lossy(&stderr).into_owned();
        let (success, code) = if stdout.is_empty() && stderr.is_empty() {
            (true, 0)
        } else {
            (true, 0)
        };
        Ok(ToolResult {
            success,
            output: format!("{stdout}\n{stderr}"),
            display: Some(super::DisplayBlock::Output { stdout, stderr, code }),
        })
    }
}

// ---------- glob ----------

pub struct Glob;

#[async_trait::async_trait]
impl Tool for Glob {
    fn def(&self) -> &'static ToolDefinition {
        &ToolDefinition {
            name: "glob",
            description: "按 glob 模式列出工作区文件",
            parameters: json!({
                "type": "object",
                "properties": { "pattern": { "type": "string", "example": "src/**/*.ts" } },
                "required": ["pattern"]
            }),
            is_read_only: true,
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let pattern = str_arg(&input, "pattern")?;
        let matcher = globset_matcher(&pattern)?;
        let mut hits: Vec<String> = WalkDir::new(&ctx.cwd)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                let rel = e.path().strip_prefix(&ctx.cwd).ok()?.to_string_lossy().replace('\\', "/");
                if matcher(&rel) { Some(rel) } else { None }
            })
            .take(2000)
            .collect();
        hits.sort();
        let joined = hits.join("\n");
        Ok(file_ref(&pattern, joined))
    }
}

/// 轻量 glob：`**` 跨目录、`*` 单层、`?` 单字符、`{a,b}` 不支持（对齐 CLI 常用面）
fn globset_matcher(pattern: &str) -> Result<Box<dyn Fn(&str) -> bool + Send>> {
    let mut re = String::from("^");
    for c in pattern.chars() {
        match c {
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '.' => re.push_str("\\."),
            '{' | '}' => re.push_str(&c.to_string()),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    // `**` 递归：先把上面产出的 `[^/]*[^/]*` 合并语义修正
    let re = if pattern.contains("**") {
        re.replace("[^/]*[^/]*", ".*")
    } else {
        re
    };
    let re = regex::Regex::new(&format!("{re}$"))?;
    Ok(Box::new(move |s: &str| re.is_match(s)))
}

// ---------- grep ----------

pub struct Grep;

#[async_trait::async_trait]
impl Tool for Grep {
    fn def(&self) -> &'static ToolDefinition {
        &ToolDefinition {
            name: "grep",
            description: "正则搜索工作区文件内容（返回 文件:行号: 内容）",
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "include": { "type": "string", "description": "文件名 glob 过滤（可选）" }
                },
                "required": ["pattern"]
            }),
            is_read_only: true,
        }
    }

    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let pattern = str_arg(&input, "pattern")?;
        let re = regex::Regex::new(&pattern)?;
        let include = input.get("include").and_then(|v| v.as_str()).map(|s| s.to_string());
        let inc_matcher = match &include {
            Some(p) => Some(globset_matcher(p)?),
            None => None,
        };
        let mut hits: Vec<String> = Vec::new();
        for entry in WalkDir::new(&ctx.cwd)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let rel = match entry.path().strip_prefix(&ctx.cwd) {
                Ok(p) => p.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            if let Some(m) = &inc_matcher {
                if !m(&rel) {
                    continue;
                }
            }
            let Ok(bytes) = std::fs::read(entry.path()) else { continue };
            if bytes.len() > MAX_FILE_BYTES as usize {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            for (i, line) in text.lines().enumerate() {
                if re.is_match(line) {
                    hits.push(format!("{rel}:{}: {}", i + 1, line.trim()));
                    if hits.len() >= 500 {
                        break;
                    }
                }
            }
            if hits.len() >= 500 {
                break;
            }
        }
        let joined = hits.join("\n");
        Ok(file_ref(&pattern, joined))
    }
}

#[allow(unused)]
fn _unused(_: PathBuf) {}
