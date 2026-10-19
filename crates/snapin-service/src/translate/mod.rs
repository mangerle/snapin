//! 机器翻译与多语言转换服务模块。

pub mod engine;
pub mod providers;
pub mod types;

pub use engine::Translator;
pub use providers::create_translator;
pub use types::{TranslateOptions, TranslateResult, TranslationItem};
