//! 统一光栅化渲染与呈现基础设施模块。

pub mod surface;
pub mod text;

pub use surface::blit_pixmap_to_surface;
pub use text::{with_thread_local_text_renderer, TextRenderer};
