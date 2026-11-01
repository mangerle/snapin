//! 机器翻译与原位覆盖渲染配置模块。

use crate::config::enums::{CoverMode, TargetLanguage, TranslateProvider};
use serde::{Deserialize, Serialize};
use std::fmt;

/// 机器翻译核心配置
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslateConfig {
    /// 翻译服务商
    #[serde(default)]
    pub provider: TranslateProvider,

    /// 翻译目标语言
    #[serde(default)]
    pub target_language: TargetLanguage,

    /// 截图原位背景擦除模式
    #[serde(default)]
    pub cover_mode: CoverMode,

    /// 各第三方翻译服务商凭证配置
    #[serde(default)]
    pub api_keys: TranslateApiKeys,
}

/// 第三方翻译服务商认证凭证集合
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslateApiKeys {
    /// 百度翻译 App ID
    #[serde(default)]
    pub baidu_app_id: String,
    /// 百度翻译 密钥
    #[serde(default)]
    pub baidu_secret_key: String,

    /// 网易有道 App Key
    #[serde(default)]
    pub youdao_app_key: String,
    /// 网易有道 App Secret
    #[serde(default)]
    pub youdao_app_secret: String,

    /// 阿里云 AccessKey ID
    #[serde(default)]
    pub aliyun_access_key_id: String,
    /// 阿里云 AccessKey Secret
    #[serde(default)]
    pub aliyun_access_key_secret: String,
}

impl fmt::Debug for TranslateApiKeys {
    /// 调试输出只暴露“是否已配置”，绝不打印任何翻译服务凭证明文。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TranslateApiKeys")
            .field("baidu_app_id", &redacted(&self.baidu_app_id))
            .field("baidu_secret_key", &redacted(&self.baidu_secret_key))
            .field("youdao_app_key", &redacted(&self.youdao_app_key))
            .field("youdao_app_secret", &redacted(&self.youdao_app_secret))
            .field(
                "aliyun_access_key_id",
                &redacted(&self.aliyun_access_key_id),
            )
            .field(
                "aliyun_access_key_secret",
                &redacted(&self.aliyun_access_key_secret),
            )
            .finish()
    }
}

/// 将非空凭证替换为固定占位符，空值保持空字符串以便区分“未配置”。
fn redacted(value: &str) -> &'static str {
    if value.is_empty() {
        ""
    } else {
        "<已隐藏>"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 调试输出不得包含任何非空翻译凭证明文，只保留是否配置的占位状态。
    #[test]
    fn test_debug_redacts_all_translate_secrets() {
        let keys = TranslateApiKeys {
            baidu_app_id: "baidu-id-secret".to_string(),
            baidu_secret_key: "baidu-key-secret".to_string(),
            youdao_app_key: "youdao-key-secret".to_string(),
            youdao_app_secret: "youdao-secret-secret".to_string(),
            aliyun_access_key_id: "aliyun-id-secret".to_string(),
            aliyun_access_key_secret: "aliyun-secret-secret".to_string(),
        };
        let debug = format!("{keys:?}");
        for secret in [
            "baidu-id-secret",
            "baidu-key-secret",
            "youdao-key-secret",
            "youdao-secret-secret",
            "aliyun-id-secret",
            "aliyun-secret-secret",
        ] {
            let leaked = debug.contains(secret);
            assert!(!leaked, "调试输出不得泄露翻译凭证明文");
        }
        assert!(debug.contains("<已隐藏>"));
    }

    /// 空凭证保持空字符串，便于区分“未配置”和“已配置但被脱敏”。
    #[test]
    fn test_debug_keeps_empty_credentials_visible_as_empty() {
        let debug = format!("{:?}", TranslateApiKeys::default());
        assert!(debug.contains("baidu_app_id: \"\""));
    }
}
