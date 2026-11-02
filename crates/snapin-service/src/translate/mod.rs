//! 机器翻译与多语言转换服务模块。

pub(crate) mod batch;
mod config;
mod engine;
mod http;
mod language;
pub(crate) mod providers;

pub use config::{TranslateProvider, TranslatorOptions};

/// 合并文本批次的翻译请求。
pub(crate) struct BatchTextRequest<'a> {
    pub(crate) texts: &'a [String],
    pub(crate) source_lang: &'a str,
    pub(crate) target_lang: &'a str,
}

/// 单次合并请求的文本、语言与预期返回条数。
pub(crate) struct JoinedTextRequest<'a> {
    pub(crate) query: &'a str,
    pub(crate) source_lang: &'a str,
    pub(crate) target_lang: &'a str,
    pub(crate) expected: usize,
}

/// 指定端点的批量翻译请求。
pub(crate) struct EndpointBatchRequest<'a> {
    pub(crate) endpoint: &'a str,
    pub(crate) batch: BatchTextRequest<'a>,
}
pub use engine::Translator;
pub use providers::create_translator;
