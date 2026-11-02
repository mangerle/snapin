//! OCR 文字识别配置模块。

use serde::{Deserialize, Serialize};

/// OCR 文字识别核心配置
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrConfig {
    /// 本地模型配置
    #[serde(default)]
    pub local: LocalModelConfig,
}

/// 本地离线模型路径配置
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelConfig {
    /// 自定义模型存储目录 (留空则使用系统标准 /usr/share 与 XDG 数据路径)
    #[serde(default)]
    pub model_dir: String,
}
