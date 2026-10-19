//! 字符检测与文字识别子系统。

pub mod config;
pub mod engine;
pub mod types;

pub use config::{load_character_dict, LocalOcrPaths, ModelInferenceConfig};
pub use engine::OcrEngine;
pub use types::{OcrResult, OcrTextBlock};
