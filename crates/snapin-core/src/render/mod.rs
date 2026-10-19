//! 统一光栅化渲染与呈现基础设施模块。

pub mod text;

pub use text::{with_thread_local_text_renderer, TextBoxOptions, TextRenderer};
