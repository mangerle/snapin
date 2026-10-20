//! 翻译服务管理器与工厂构造。

use crate::translate::config::{TranslateProvider, TranslatorOptions};
use crate::translate::engine::Translator;
use crate::translate::providers::{
    aliyun::AliyunTranslator, baidu::BaiduTranslator, bing_free::BingFreeTranslator,
    google_free::GoogleFreeTranslator, youdao::YoudaoTranslator,
};
use std::sync::Arc;

/// 构建目标翻译服务适配器实例
pub fn create_translator(options: &TranslatorOptions) -> Arc<dyn Translator> {
    match options.provider {
        TranslateProvider::BingFree => Arc::new(BingFreeTranslator::new()),
        TranslateProvider::GoogleFree => Arc::new(GoogleFreeTranslator::new()),
        TranslateProvider::Baidu => Arc::new(BaiduTranslator::new(
            options.baidu_app_id.clone(),
            options.baidu_secret_key.clone(),
        )),
        TranslateProvider::Youdao => Arc::new(YoudaoTranslator::new(
            options.youdao_app_key.clone(),
            options.youdao_app_secret.clone(),
        )),
        TranslateProvider::Aliyun => Arc::new(AliyunTranslator::new(
            options.aliyun_access_key_id.clone(),
            options.aliyun_access_key_secret.clone(),
        )),
    }
}
