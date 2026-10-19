//! 机器翻译与多语言转换服务模块。

pub mod engine;
pub mod types;

pub use engine::Translator;
pub use types::{TranslateOptions, TranslateResult, TranslationItem};
