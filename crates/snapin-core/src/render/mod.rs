//! 统一光栅化渲染与呈现基础设施模块。

pub mod beautify;
mod layout;
mod measure;
mod raster;
pub mod text;

pub use beautify::{render_beautified, BeautifyBackground, BeautifyOptions, SocialSize};
pub use layout::CenteredTextOptions;
pub use text::{with_thread_local_text_renderer, TextBoxOptions, TextRenderer};
