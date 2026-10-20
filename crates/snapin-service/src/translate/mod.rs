//! 机器翻译与多语言转换服务模块。

pub mod config;
pub mod engine;
pub mod http;
pub mod language;
pub mod providers;

pub use config::{TranslateProvider, TranslatorOptions};
pub use engine::Translator;
pub use providers::create_translator;
