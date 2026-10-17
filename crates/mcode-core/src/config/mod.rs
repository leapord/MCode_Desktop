//! config.json — 唯一事实源，与 CLI 版逐字段互通（对齐 mcode/src/config/config.ts）。
//! 桌面扩展配置（权限规则 / MCP / hooks）放 ~/.mcode/desktop.json，不污染 CLI 互操作面。

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use crate::permission::Mode;

// ---------- CLI 互通 schema（对齐 McodeConfig，逐字段一致） ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    pub name: String,
    pub base_url: String,
    /// 环境变量名；若不是合法变量名则按明文 key 兜底（对齐 CLI resolveApiKey）
    pub api_key_env: String,
    pub models: Vec<String>,
    /// 接受图片输入的模型名单；不在名单内的模型拒绝图片附件并降级文本
    pub multimodal_models: Vec<String>,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_url: String::new(),
            api_key_env: String::new(),
            models: vec![],
            multimodal_models: vec![],
        }
    }
}

impl Provider {
    /// 解析 API key：优先环境变量，非合法变量名则按明文兜底（对齐 config.ts resolveApiKey）
    pub fn resolve_api_key(&self) -> Option<String> {
        let v = self.api_key_env.as_str();
        if v.is_empty() {
            return None;
        }
        if let Ok(from_env) = std::env::var(v) {
            if !from_env.is_empty() {
                return Some(from_env);
            }
        }
        let is_env_name = {
            let mut cs = v.chars();
            match cs.next() {
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
                }
                _ => false,
            }
        };
        if !is_env_name {
            Some(v.to_string())
        } else {
            None
        }
    }

    pub fn is_multimodal(&self, model: &str) -> bool {
        self.multimodal_models.iter().any(|m| m == model)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[serde(rename = "zh_CN")]
    ZhCn,
    #[serde(rename = "zh_TW")]
    ZhTw,
    #[serde(rename = "en_US")]
    EnUs,
    #[serde(rename = "ja_JP")]
    JaJp,
    #[serde(rename = "ko_KR")]
    KoKr,
}

impl Default for Language {
    fn default() -> Self {
        Self::ZhCn
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// PUA 收敛纪律指令每个会话常驻（/pua:on）
    pub pua_always_on: bool,
    pub language: Language,
    pub active_provider: Option<String>,
    /// 当前 provider 内选中的模型
    pub active_model: Option<String>,
    pub providers: Vec<Provider>,
    pub default_mode: Mode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pua_always_on: false,
            language: Language::default(),
            active_provider: None,
            active_model: None,
            providers: vec![],
            default_mode: Mode::Passon,
        }
    }
}

impl Config {
    /// 读取 config.json；文件不存在返回默认骨架（首次启动 → 引导流程）
    pub fn load() -> Self {
        Self::load_from(&crate::util::config_path())
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&crate::util::config_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn active_provider(&self) -> Option<&Provider> {
        let name = self.active_provider.as_ref()?;
        self.providers.iter().find(|p| &p.name == name)
    }

    /// 当前激活模型名（对齐 resolveActiveModel：activeModel 优先，回落 provider 首个）
    pub fn active_model(&self) -> Option<String> {
        if let Some(m) = self.active_model.as_deref() {
            if !m.is_empty() {
                return Some(m.to_string());
            }
        }
        self.active_provider().and_then(|p| p.models.first().cloned())
    }

    /// 首次启动判定：文件不存在或 providers 为空（对齐 CLI 强制检查）
    pub fn needs_onboarding(&self) -> bool {
        self.providers.is_empty()
    }

    /// 供应商就绪检查（每回合评估，对齐 providerIssue：永不缓存）
    pub fn provider_ready(&self) -> Result<()> {
        let Some(p) = self.active_provider() else {
            anyhow::bail!("未配置模型供应商，请先完成引导（设置 → 供应商）");
        };
        if p.resolve_api_key().is_none() {
            anyhow::bail!(
                "供应商 {} 的 API key 不可用：环境变量 {} 未设置且值不是明文 key",
                p.name,
                p.api_key_env
            );
        }
        if self.active_model().is_none() {
            anyhow::bail!("未选择模型");
        }
        Ok(())
    }
}

// ---------- 桌面扩展配置（~/.mcode/desktop.json，CLI 不读取） ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopConfig {
    /// 权限规则（allow/deny，/permissions 面板数据源）
    pub permission_rules: Vec<PermissionRule>,
    /// MCP server 进程配置（stdio transport）
    pub mcp_servers: Vec<McpServerConfig>,
    /// hooks 生命周期配置
    pub hooks: Vec<HookConfig>,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self { permission_rules: vec![], mcp_servers: vec![], hooks: vec![] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    pub tool: String,
    /// 子串匹配模式，如 `bash(rm *)` 或 `edit_file(src/**)`
    pub pattern: String,
    pub allow: bool,
}

/// MCP stdio server：进程命令 + 参数 + 环境注入
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// 是否随应用启动自动连接
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// hook 规则：事件 + 工具匹配器 + shell 命令
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookConfig {
    pub event: HookEvent,
    /// 工具名匹配（`*` 全部，否则子串匹配）
    pub matcher: String,
    pub command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Stop,
}

impl DesktopConfig {
    pub fn load() -> Self {
        Self::load_from(&crate::util::desktop_config_path())
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = crate::util::desktop_config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_cli_config() {
        // 对齐本机真实 ~/.mcode/config.json 的字段形态
        let raw = r#"{
            "language": "zh_CN",
            "activeProvider": "glm",
            "activeModel": "glm-5.3-flash",
            "providers": [{
                "name": "glm",
                "baseURL": "https://open.bigmodel.cn/api/v1",
                "apiKeyEnv": "MY_KEY_ENV",
                "models": ["glm-5.3-flash"],
                "multimodalModels": []
            }],
            "defaultMode": "passon",
            "puaAlwaysOn": true
        }"#;
        let cfg: Config = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.active_provider.as_deref(), Some("glm"));
        assert_eq!(cfg.active_model().as_deref(), Some("glm-5.3-flash"));
        assert!(cfg.pua_always_on);
        assert_eq!(cfg.providers[0].models, vec!["glm-5.3-flash".to_string()]);
        // 序列化回 CLI 可读的 camelCase
        let out = serde_json::to_string(&cfg).unwrap();
        assert!(out.contains("\"activeProvider\""));
        assert!(out.contains("\"puaAlwaysOn\""));
        assert!(out.contains("\"multimodalModels\""));
    }

    #[test]
    fn api_key_env_then_literal_fallback() {
        let mut p = Provider {
            name: "t".into(),
            base_url: "http://x".into(),
            api_key_env: "MCODE_TEST_KEY_ENV".into(),
            models: vec![],
            multimodal_models: vec![],
        };
        // SAFETY: 测试进程单线程设置环境变量，无并发读取
        unsafe { std::env::set_var("MCODE_TEST_KEY_ENV", "from-env") };
        assert_eq!(p.resolve_api_key().as_deref(), Some("from-env"));
        // SAFETY: 同上
        unsafe { std::env::remove_var("MCODE_TEST_KEY_ENV") };
        // 合法变量名但未设置 → None
        assert_eq!(p.resolve_api_key(), None);
        // 非合法变量名 → 明文兜底
        p.api_key_env = "sk-literal-key".into();
        assert_eq!(p.resolve_api_key().as_deref(), Some("sk-literal-key"));
    }

    #[test]
    fn onboarding_detection() {
        assert!(Config::default().needs_onboarding());
        let mut c = Config::default();
        c.providers.push(Provider::default());
        assert!(!c.needs_onboarding());
    }
}
