//! OCR 文字识别配置模块。

use crate::config::enums::OcrEngineKind;
use serde::{Deserialize, Serialize};

/// OCR 文字识别核心配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrConfig {
    /// OCR 引擎类型
    #[serde(default)]
    pub engine: OcrEngineKind,

    /// 是否跳过文本方向分类 (桌面常规截屏绝大多数为水平文字，跳过可提升 30% 速度)
    #[serde(default = "default_skip_orientation")]
    pub skip_orientation_classification: bool,

    /// 本地模型配置
    #[serde(default)]
    pub local: LocalModelConfig,
}

fn default_skip_orientation() -> bool {
    true
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            engine: OcrEngineKind::default(),
            skip_orientation_classification: default_skip_orientation(),
            local: LocalModelConfig::default(),
        }
    }
}

/// 本地离线模型路径配置
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelConfig {
    /// 自定义模型存储目录 (留空则使用系统标准 /usr/share 与 XDG 数据路径)
    #[serde(default)]
    pub model_dir: String,
}
