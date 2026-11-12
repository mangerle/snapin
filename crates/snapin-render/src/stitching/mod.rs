//! 长截图连续帧图像拼接引擎与位移估计。

mod canvas_ops;
mod crop;
mod history;
mod matcher;
mod overlay;
mod paged_canvas;
mod pyramid;
mod stitcher;
mod thumbnail;
mod types;

#[cfg(test)]
mod bidirectional_tests;
#[cfg(test)]
mod consensus_tests;
#[cfg(test)]
mod paged_canvas_tests;
#[cfg(test)]
mod tests;

pub use matcher::estimate_shift;
pub use paged_canvas::PagedCanvasSnapshot;
pub use stitcher::{analyze_frame_shift, LongCaptureStitcher};
pub use thumbnail::{render_thumbnail_overview, ThumbnailRenderSpec};
pub use types::{
    FrameShiftAnalysis, MatchResult, StitchDirection, StitchFrameOutcome, StitchOptions,
    ThumbnailOverview, ViewportWindow, DEFAULT_MAX_CANVAS_EXTENT, DEFAULT_MAX_CANVAS_PIXELS,
    DEFAULT_MIN_THUMBNAIL_CROSS, MIN_CROPPED_SPAN,
};
