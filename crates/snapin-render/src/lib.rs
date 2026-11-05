//! Snapin 文本排版、光栅化与截图美化渲染。

pub mod beautify;
mod error;
mod layout;
mod measure;
mod raster;
pub mod text;

pub use beautify::{render_beautified, BeautifyOptions};
pub use error::RenderError;
pub use layout::CenteredTextOptions;
pub use snapin_config::config::{BeautifyBackground, SocialSize};
pub use text::{
    with_thread_local_text_renderer, StyledTextDrawOptions, TextBoxOptions, TextDrawOptions,
    TextMeasureOptions, TextRenderer, TextStyleOptions,
};
