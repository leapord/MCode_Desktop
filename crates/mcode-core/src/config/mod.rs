//! config.json — 唯一事实源，与 CLI 版逐字段互通（对齐 MCode.md 2.5）

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use crate::permission::Mode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelConfig {
    pub name: String,
    /// 多模态标记：非 true 时图片附件拒绝上传，降级为文本提示
    pub multimodal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    pub name: String,
    pub base_url: String,
    /// apiKey 一律存环境变量名，不落明文
    pub api_key_env: String,
    pub models: Vec<ModelConfig>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self { name: String::new(), multimodal: false }
    }
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_url: String::new(),
            api_key_env: String::new(),
            models: vec![],
        }
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
    pub language: Language,
    pub active_provider: Option<String>,
    pub providers: Vec<Provider>,
    pub default_mode: Mode,
    /// 会话内「总是允许」沉淀的规则（对齐 /permissions allow/deny）
    pub permission_rules: Vec<PermissionRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    pub tool: String,
    /// glob 模式，如 `bash(rm *)` 或 `edit_file(src/**)`
    pub pattern: String,
    pub allow: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: Language::default(),
            active_provider: None,
            providers: vec![],
            default_mode: Mode::Passon,
            permission_rules: vec![],
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

    /// 首次启动判定：文件不存在或 providers 为空（对齐 CLI 强制检查）
    pub fn needs_onboarding(&self) -> bool {
        self.providers.is_empty()
    }

    /// 当前激活模型（含 multimodal 标记）
    pub fn active_model(&self) -> Option<&ModelConfig> {
        self.active_provider()?.models.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_camel_case_compatible_with_cli() {
        let cfg = Config {
            active_provider: Some("zhipu".into()),
            providers: vec![Provider {
                name: "zhipu".into(),
                base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
                api_key_env: "ZHIPU_API_KEY".into(),
                models: vec![ModelConfig { name: "glm-4.7-multimodal".into(), multimodal: true }],
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"activeProvider\":\"zhipu\""));
        assert!(json.contains("\"multimodal\":true"));
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.active_provider.as_deref(), Some("zhipu"));
    }

    #[test]
    fn onboarding_detection() {
        assert!(Config::default().needs_onboarding());
        let mut c = Config::default();
        c.providers.push(Provider::default());
        assert!(!c.needs_onboarding());
    }
}
