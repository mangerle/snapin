//! Snapin 核心领域模型与算法基建 crate。
//!
//! 提供错误定义、配置模型、软光栅文本渲染排版以及矢量标注图元引擎。

pub mod annotation;
pub mod config;
pub mod error;
pub mod paths;
pub mod render;
pub mod updater;

pub use annotation::{AnnotationAction, AnnotationHistory, AnnotationShape};
pub use config::{Config, HotkeyConfig, OutputConfig, PasterConfig, SnipConfig};
pub use error::CoreError;
pub use paths::AppPaths;
pub use render::{with_thread_local_text_renderer, TextRenderer};
pub use updater::{
    build_updater, check_and_recover_on_startup, check_update, check_update_async,
    confirm_update_success, DownloadProgress, UpdateInfo, DEFAULT_MANIFEST_URL, UPDATE_PUBLIC_KEY,
};
