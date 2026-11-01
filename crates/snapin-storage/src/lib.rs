//! 图像文件存储、格式化模板、选区缓存与原生像素载荷校验。
//!
//! 本 crate 只负责持久化与编码，不包含平台窗口、剪贴板或 D-Bus 逻辑；
//! 平台适配层与应用层按需依赖本 crate。

pub mod error;

pub mod raw_image;
pub mod selection_cache;
pub mod template;
pub mod writer;

pub use error::StorageError;
pub use raw_image::{checked_rgba_byte_len, MAX_RAW_PIXELS};
pub use selection_cache::{clear_selection_cache, load_last_selection, save_last_selection};
pub use template::{expand_home_dir, format_template};
pub use writer::{resolve_save_path, save_image_to_path, save_image_with_config};
