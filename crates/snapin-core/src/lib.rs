//! Snapin 核心领域模型与算法基建 crate。
//!
//! 提供错误定义、配置模型、软光栅文本渲染排版以及矢量标注图元引擎。

pub mod annotation;
pub mod config;
pub mod error;
pub mod render;

pub use annotation::{AnnotationAction, AnnotationHistory, AnnotationShape};
pub use config::{Config, HotkeyConfig, OutputConfig, PasterConfig, SnipConfig};
pub use error::{CoreError, SnapinError};
pub use render::{with_thread_local_text_renderer, TextRenderer};
