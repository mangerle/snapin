//! 用户界面语言基础类型。

use serde::{Deserialize, Serialize};

/// 用户界面语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiLanguage {
    /// 简体中文
    #[default]
    ZhCn,
    /// 英文
    EnUs,
}

impl UiLanguage {
    /// Slint 与用户文案选择使用的稳定语言代码。
    pub const fn code(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en",
        }
    }

    /// Slint 打包翻译选择码；中文是源语言，使用空字符串恢复默认资源。
    pub const fn slint_translation_code(self) -> &'static str {
        match self {
            Self::ZhCn => "",
            Self::EnUs => "en",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UiLanguage;

    /// 中文是 Slint 源语言；设置中心必须使用空翻译码，避免查询不存在的 zh-CN 资源。
    #[test]
    fn test_slint_translation_code_uses_source_language_for_chinese() {
        assert_eq!(UiLanguage::ZhCn.slint_translation_code(), "");
        assert_eq!(UiLanguage::ZhCn.code(), "zh-CN");
        assert_eq!(UiLanguage::EnUs.slint_translation_code(), "en");
    }
}
