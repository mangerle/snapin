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
#[derive(Debug, Clone, Default)]
pub struct TranslatorOptions {
    pub provider: TranslateProvider,
    pub baidu_app_id: String,
    pub baidu_secret_key: String,
    pub youdao_app_key: String,
    pub youdao_app_secret: String,
    pub aliyun_access_key_id: String,
    pub aliyun_access_key_secret: String,
}
