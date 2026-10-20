//! 翻译服务管理器与工厂构造。

use crate::translate::engine::Translator;
use crate::translate::providers::{
    aliyun::AliyunTranslator, baidu::BaiduTranslator, bing_free::BingFreeTranslator,
    google_free::GoogleFreeTranslator, youdao::YoudaoTranslator,
};
use snapin_core::config::{TranslateConfig, TranslateProvider};
use std::sync::Arc;

/// 构建目标翻译服务适配器实例
pub fn create_translator(config: &TranslateConfig) -> Arc<dyn Translator> {
    match config.provider {
        TranslateProvider::GoogleFree => Arc::new(GoogleFreeTranslator::new()),
        TranslateProvider::Baidu => Arc::new(BaiduTranslator::new(
            config.api_keys.baidu_app_id.clone(),
            config.api_keys.baidu_secret_key.clone(),
        )),
        TranslateProvider::Youdao => Arc::new(YoudaoTranslator::new(
            config.api_keys.youdao_app_key.clone(),
            config.api_keys.youdao_app_secret.clone(),
        )),
        TranslateProvider::Aliyun => Arc::new(AliyunTranslator::new(
            config.api_keys.aliyun_access_key_id.clone(),
            config.api_keys.aliyun_access_key_secret.clone(),
        )),
        TranslateProvider::BingFree => Arc::new(BingFreeTranslator::new()),
    }
}
