//! Snapin 核心能力门面。
//!
//! 具体实现已按依赖与职责拆分为基础契约、配置、渲染和标注四个 crate。本 crate
//! 只保留稳定的聚合路径，供同时需要多个核心领域的应用层使用；轻量消费者应直接
//! 依赖所需子 crate，避免无谓引入 Tiny-Skia 与 Cosmic-Text。

pub use snapin_annotation as annotation;
pub use snapin_config::config;
pub use snapin_foundation::{error, geometry, i18n, paths, trigger};
pub use snapin_render as render;

pub use annotation::{AnnotationHistory, AnnotationShape};
pub use error::CoreError;
pub use geometry::Rect;
pub use paths::AppPaths;
pub use render::{with_thread_local_text_renderer, TextRenderer};
pub use snapin_config::config::{
    BeautifyConfig, Config, HotkeyConfig, OutputConfig, PasterConfig, SnipConfig,
};
pub use trigger::{PostCaptureAction, SnipMode};

pub use snapin_foundation::trf;
