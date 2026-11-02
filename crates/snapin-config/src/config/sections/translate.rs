//! 机器翻译与原位覆盖渲染配置模块。

use crate::config::enums::{CoverMode, TargetLanguage, TranslateProvider};
use crate::config::types::SecretString;
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslateApiKeys {
    /// 百度翻译 App ID
    #[serde(default)]
    pub baidu_app_id: SecretString,
    /// 百度翻译 密钥
    #[serde(default)]
    pub baidu_secret_key: SecretString,

    /// 网易有道 App Key
    #[serde(default)]
    pub youdao_app_key: SecretString,
    /// 网易有道 App Secret
    #[serde(default)]
    pub youdao_app_secret: SecretString,

    /// 阿里云 AccessKey ID
    #[serde(default)]
    pub aliyun_access_key_id: SecretString,
    /// 阿里云 AccessKey Secret
    #[serde(default)]
    pub aliyun_access_key_secret: SecretString,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 调试输出不得包含任何非空翻译凭证明文，只保留是否配置的占位状态。
    #[test]
    fn test_debug_redacts_all_translate_secrets() {
        let keys = TranslateApiKeys {
            baidu_app_id: SecretString::from("baidu-id-secret"),
            baidu_secret_key: SecretString::from("baidu-key-secret"),
            youdao_app_key: SecretString::from("youdao-key-secret"),
            youdao_app_secret: SecretString::from("youdao-secret-secret"),
            aliyun_access_key_id: SecretString::from("aliyun-id-secret"),
            aliyun_access_key_secret: SecretString::from("aliyun-secret-secret"),
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
