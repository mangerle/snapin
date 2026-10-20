//! 机器翻译与原位覆盖渲染配置模块。

use crate::config::enums::{CoverMode, TargetLanguage, TranslateProvider};
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
