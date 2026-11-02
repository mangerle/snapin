//! 机器翻译与多语言转换服务模块。

pub(crate) mod batch;
mod config;
mod engine;
mod http;
mod language;
pub(crate) mod providers;

pub use config::{TranslateProvider, TranslatorOptions};
pub use engine::Translator;
pub use providers::create_translator;
