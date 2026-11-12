//! 长截图连续帧图像拼接引擎与位移估计。

mod matcher;
mod stitcher;
mod thumbnail;
mod types;

#[cfg(test)]
mod tests;

pub use matcher::estimate_shift;
pub use stitcher::LongCaptureStitcher;
pub use types::{
    MatchResult, StitchDirection, StitchFrameOutcome, StitchOptions, ThumbnailOverview,
    ViewportWindow, DEFAULT_MIN_THUMBNAIL_CROSS, MIN_CROPPED_SPAN,
};
