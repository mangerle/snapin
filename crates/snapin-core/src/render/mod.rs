//! 统一光栅化渲染与呈现基础设施模块。

mod layout;
mod measure;
mod raster;
pub mod text;

pub use layout::CenteredTextOptions;
pub use text::{with_thread_local_text_renderer, TextBoxOptions, TextRenderer};
