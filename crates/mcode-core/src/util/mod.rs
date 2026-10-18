//! 通用工具：数据目录、token 统计

pub mod file_refs;

use std::path::PathBuf;

/// 与 CLI 版完全互通的数据根目录：`~/.mcode/`
pub fn mcode_dir() -> PathBuf {
    dirs::home_dir()
        .expect("无法获取 HOME 目录")
        .join(".mcode")
}

pub fn config_path() -> PathBuf {
    mcode_dir().join("config.json")
}

/// 桌面扩展配置（权限规则 / MCP / hooks）— CLI 不读取，避免污染互操作面
pub fn desktop_config_path() -> PathBuf {
    mcode_dir().join("desktop.json")
}

/// 项目级 MCODE.md 记忆文档搜索 + 全局，均存在时拼接
pub fn mcode_md_paths(cwd: &std::path::Path) -> Vec<PathBuf> {
    let mut out = vec![];
    // 最近优先：cwd → 根（对齐 CLI mdoc.ts 的 nearest-first walk）
    let mut cur = Some(cwd.to_path_buf());
    while let Some(d) = cur {
        out.push(d.join("MCODE.md"));
        cur = d.parent().map(|p| p.to_path_buf());
    }
    out.push(mcode_dir().join("MCODE.md"));
    out
}

pub fn sessions_dir(cwd: &std::path::Path) -> PathBuf {
    let hash = format!("{:x}", md5_of(cwd.to_string_lossy().as_bytes()));
    mcode_dir().join("projects").join(hash).join("sessions")
}

fn md5_of(bytes: &[u8]) -> u64 {
    // 简易 FNV-1a：与 CLI 的 hash-of-pwd 语义对齐（目录标识，非安全用途）
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// token 用量累计（按模型分组，对齐 CLI util/token-usage.ts）
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenLedger {
    /// model name -> usage
    pub by_model: std::collections::BTreeMap<String, TokenUsage>,
}

impl TokenLedger {
    pub fn record(&mut self, model: &str, u: TokenUsage) {
        let e = self.by_model.entry(model.to_string()).or_default();
        e.input += u.input;
        e.output += u.output;
        e.cache_read += u.cache_read;
        e.cache_write += u.cache_write;
    }

    pub fn total(&self) -> TokenUsage {
        self.by_model.values().fold(TokenUsage::default(), |a, u| TokenUsage {
            input: a.input + u.input,
            output: a.output + u.output,
            cache_read: a.cache_read + u.cache_read,
            cache_write: a.cache_write + u.cache_write,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_groups_by_model() {
        let mut l = TokenLedger::default();
        l.record("glm-4.7", TokenUsage { input: 100, output: 10, ..Default::default() });
        l.record("glm-4.7", TokenUsage { input: 50, output: 5, ..Default::default() });
        l.record("glm-4.7-air", TokenUsage { input: 7, ..Default::default() });
        assert_eq!(l.by_model["glm-4.7"].input, 150);
        assert_eq!(l.total().output, 15);
    }
}
