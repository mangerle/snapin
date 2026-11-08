//! 长截图连续帧图像拼接引擎与位移估计。

mod matcher;
mod stitcher;
mod types;

#[cfg(test)]
mod tests;

pub use matcher::estimate_shift;
pub use stitcher::LongCaptureStitcher;
pub use types::{MatchResult, StitchDirection, StitchOptions, ViewportWindow};
