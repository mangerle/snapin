//! 翻译数据契约与语言选项定义。

use serde::{Deserialize, Serialize};

/// 翻译请求选项
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslateOptions {
    /// 源语言代码 (如 "auto", "zh", "en", "ja", "ko")
    pub from: String,
    /// 目标语言代码 (如 "zh", "en", "ja", "ko")
    pub to: String,
}

impl Default for TranslateOptions {
    fn default() -> Self {
        Self {
            from: "auto".to_string(),
            to: "zh".to_string(),
        }
    }
}

/// 单个原位文本块的翻译替换元数据
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranslationItem {
    /// 文本框在选区内的局部几何区域 [x, y, width, height]
    pub rect: (f32, f32, f32, f32),
    /// 识别原文
    pub original_text: String,
    /// 翻译结果译文
    pub translated_text: String,
    /// 背景擦除颜色 RGBA
    pub bg_color: (u8, u8, u8, u8),
    /// 译文字体颜色 RGBA
    pub text_color: (u8, u8, u8, u8),
    /// 计算适配后的字号大小 (pt)
    pub font_size: f32,
}

/// 批量翻译处理结果
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TranslateResult {
    /// 所有翻译就绪的原位图元项
    pub items: Vec<TranslationItem>,
}
