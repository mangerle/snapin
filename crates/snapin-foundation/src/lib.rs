//! Snapin 无重型 UI 依赖的基础契约。

pub mod beautify;
pub mod error;
pub mod file;
pub mod geometry;
pub mod i18n;
pub mod language;
pub mod paths;
pub mod translation;
pub mod trigger;

pub use beautify::{BeautifyBackground, BeautifyOptions, SocialSize};
pub use error::CoreError;
pub use file::{read_file_limited, read_utf8_file_limited};
pub use geometry::{Point, Rect, RectF, RectI};
pub use language::UiLanguage;
pub use paths::AppPaths;
pub use translation::CoverMode;
pub use trigger::{PostCaptureAction, SnipMode};
