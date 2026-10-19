//! 机器翻译与原位覆盖渲染配置模块。

use serde::{Deserialize, Serialize};

/// 机器翻译核心配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslateConfig {
    /// 翻译服务商，支持 "bing_free" (默认) | "google_free" | "baidu" | "youdao" | "aliyun"
    #[serde(default = "default_provider")]
    pub provider: String,

    /// 翻译目标语言代码，默认 "zh"
    #[serde(default = "default_target_language")]
    pub target_language: String,

    /// 截图原位背景擦除模式: "solid_color" (智能取色平铺) | "frosted_glass" (半透明磨砂模糊)
    #[serde(default = "default_cover_mode")]
    pub cover_mode: String,

    /// 各第三方翻译服务商凭证配置
    #[serde(default)]
    pub api_keys: TranslateApiKeys,
}

fn default_provider() -> String {
    "bing_free".to_string()
}

fn default_target_language() -> String {
    "zh".to_string()
}

fn default_cover_mode() -> String {
    "solid_color".to_string()
}

impl Default for TranslateConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            target_language: default_target_language(),
            cover_mode: default_cover_mode(),
            api_keys: TranslateApiKeys::default(),
        }
    }
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
