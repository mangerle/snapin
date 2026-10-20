//! 图像文件存储、格式化模板与选区缓存持久化子系统。

pub mod raw_image;
pub mod selection_cache;
pub mod template;
pub mod writer;

pub use raw_image::{checked_rgba_byte_len, MAX_RAW_PIXELS};
pub use selection_cache::{clear_selection_cache, load_last_selection, save_last_selection};
pub use template::{expand_home_dir, format_template};
pub use writer::{resolve_save_path, save_image_with_config};
