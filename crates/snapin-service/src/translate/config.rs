//! 翻译服务层独立配置契约。
//!
//! 服务层不依赖应用配置结构，由上层将用户配置映射为 [`TranslatorOptions`]，
//! 保持引擎库可独立测试与复用。

/// 翻译服务商类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TranslateProvider {
    /// 微软必应免费接口
    #[default]
    BingFree,
    /// 谷歌翻译免费接口
    GoogleFree,
    /// 百度翻译开放平台
    Baidu,
    /// 网易有道开放平台
    Youdao,
    /// 阿里云机器翻译
    Aliyun,
}

/// 翻译器构造参数
#[derive(Clone, Default)]
pub struct TranslatorOptions {
    pub provider: TranslateProvider,
    pub baidu_app_id: String,
    pub baidu_secret_key: String,
    pub youdao_app_key: String,
    pub youdao_app_secret: String,
    pub aliyun_access_key_id: String,
    pub aliyun_access_key_secret: String,
}

impl std::fmt::Debug for TranslatorOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TranslatorOptions")
            .field("provider", &self.provider)
            .field("baidu_app_id", &"<已隐藏>")
            .field("baidu_secret_key", &"<已隐藏>")
            .field("youdao_app_key", &"<已隐藏>")
            .field("youdao_app_secret", &"<已隐藏>")
            .field("aliyun_access_key_id", &"<已隐藏>")
            .field("aliyun_access_key_secret", &"<已隐藏>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translator_options_debug_redacts_all_credentials() {
        let options = TranslatorOptions {
            provider: TranslateProvider::Baidu,
            baidu_app_id: "secret-app-id".to_string(),
            baidu_secret_key: "secret-key".to_string(),
            youdao_app_key: "secret-youdao".to_string(),
            youdao_app_secret: "secret-youdao-key".to_string(),
            aliyun_access_key_id: "secret-aliyun-id".to_string(),
            aliyun_access_key_secret: "secret-aliyun-key".to_string(),
        };

        let debug = format!("{options:?}");
        for secret in [
            "secret-app-id",
            "secret-key",
            "secret-youdao",
            "secret-youdao-key",
            "secret-aliyun-id",
            "secret-aliyun-key",
        ] {
            assert!(!debug.contains(secret), "Debug 输出泄漏凭据: {secret}");
        }
        assert!(debug.contains("<已隐藏>"));
    }
}
