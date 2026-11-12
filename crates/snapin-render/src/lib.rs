//! Snapin 文本排版、光栅化与截图美化渲染。

pub mod beautify;
mod error;
mod layout;
mod measure;
mod raster;
pub mod stitching;
pub mod text;

pub use beautify::render_beautified;
pub use error::RenderError;
pub use layout::CenteredTextOptions;
pub use measure::{SelectionRect, TextInputMetrics};
pub use raster::{
    blit_straight_rgba_region_to_pixmap, demultiply_rgba_pixel, pixmap_into_rgba_image,
    premultiply_rgba_pixel, rgba_image_to_pixmap, BlitRgbaRegion,
};
pub use snapin_foundation::beautify::{BeautifyBackground, BeautifyOptions, SocialSize};
pub use stitching::{
    LongCaptureStitcher, MatchResult, StitchDirection, StitchFrameOutcome, StitchOptions,
    ThumbnailOverview, ViewportWindow, DEFAULT_MIN_THUMBNAIL_CROSS, MIN_CROPPED_SPAN,
};
pub use text::{
    with_thread_local_text_renderer, StyledTextDrawOptions, TextBoxOptions, TextDrawOptions,
    TextMeasureOptions, TextRenderer, TextStyleOptions,
};
