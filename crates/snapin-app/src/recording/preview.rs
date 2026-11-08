//! 录制过程中的低分辨率预览帧采样。
//!
//! 预览帧直接来自已裁剪的采集画面，因此不依赖最终容器格式，也不需要录制结束后
//! 在 GUI 线程启动外部解码器。帧数达到上限后会按固定规则隔帧抽取并扩大采样间隔，
//! 使长录制的预览仍覆盖完整时间范围，同时把内存占用限制在可控上限。

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use snapin_ports::RecordingPreviewFrame;
use std::sync::Arc;
use std::time::Duration;

const PREVIEW_MAX_EDGE: u32 = 1920;
const PREVIEW_MAX_FRAMES: usize = 36;
const PREVIEW_SAMPLE_INTERVAL: Duration = Duration::from_millis(250);
const PREVIEW_JPEG_QUALITY: u8 = 90;

/// 录制预览帧收集器。
pub(super) struct PreviewCollector {
    frames: Vec<RecordingPreviewFrame>,
    interval: Duration,
    next_sample_at: Duration,
}

impl PreviewCollector {
    /// 创建容量固定的预览收集器。
    pub(super) fn new() -> Self {
        Self {
            frames: Vec::with_capacity(PREVIEW_MAX_FRAMES),
            interval: PREVIEW_SAMPLE_INTERVAL,
            next_sample_at: Duration::ZERO,
        }
    }

    /// 判断当前录制时刻是否需要生成预览帧。
    pub(super) fn sample_due(&self, elapsed: Duration) -> bool {
        elapsed >= self.next_sample_at
    }

    /// 保存一帧已降采样的预览，并在达到上限时进行时间轴抽稀。
    pub(super) fn record(&mut self, frame: RgbaImage, elapsed: Duration) {
        let Some(frame) = encode_preview_frame(frame) else {
            tracing::debug!("录屏预览帧 JPEG 编码失败，已跳过该采样点");
            return;
        };
        if self.frames.len() >= PREVIEW_MAX_FRAMES {
            let mut retained = Vec::with_capacity(PREVIEW_MAX_FRAMES / 2 + 1);
            for frame in self.frames.drain(..).step_by(2) {
                retained.push(frame);
            }
            self.frames = retained;
            self.interval = self.interval.saturating_mul(2);
        }
        self.frames.push(frame);
        self.next_sample_at = elapsed.saturating_add(self.interval);
    }

    /// 返回最终预览帧集合。
    pub(super) fn into_frames(self) -> Vec<RecordingPreviewFrame> {
        self.frames
    }
}

/// 将采集帧按最长边限制生成预览缩略图。
pub(super) fn make_preview_frame(frame: &RgbaImage) -> RgbaImage {
    let (width, height) = frame.dimensions();
    let longest = width.max(height);
    if longest <= PREVIEW_MAX_EDGE {
        return frame.clone();
    }
    let scale = PREVIEW_MAX_EDGE as f32 / longest as f32;
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);
    image::imageops::resize(frame, target_width, target_height, FilterType::Lanczos3)
}

/// 将预览 RGBA 帧编码为 JPEG，避免高分辨率预览长期占用大量内存。
fn encode_preview_frame(frame: RgbaImage) -> Option<RecordingPreviewFrame> {
    let width = frame.width();
    let height = frame.height();
    let rgb = DynamicImage::ImageRgba8(frame).to_rgb8();
    let mut data = Vec::with_capacity((width as usize).saturating_mul(height as usize) / 4);
    JpegEncoder::new_with_quality(&mut data, PREVIEW_JPEG_QUALITY)
        .encode_image(&DynamicImage::ImageRgb8(rgb))
        .ok()?;
    Some(RecordingPreviewFrame {
        width,
        height,
        data: Arc::from(data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_thumbnail_limits_longest_edge() {
        let frame = RgbaImage::new(2560, 1440);
        let preview = make_preview_frame(&frame);
        assert_eq!(preview.width(), 1920);
        assert_eq!(preview.height(), 1080);
    }

    #[test]
    fn test_preview_collector_decimates_and_keeps_count_bounded() {
        let mut collector = PreviewCollector::new();
        let frame = RgbaImage::new(2, 2);
        let mut elapsed = Duration::ZERO;
        for _ in 0..(PREVIEW_MAX_FRAMES * 3) {
            assert!(collector.sample_due(elapsed));
            collector.record(frame.clone(), elapsed);
            elapsed = collector.next_sample_at;
        }
        assert!(collector.frames.len() <= PREVIEW_MAX_FRAMES);
        assert!(collector.interval > PREVIEW_SAMPLE_INTERVAL);
        assert!(collector.frames.iter().all(|frame| !frame.data.is_empty()));
        let decoded =
            image::load_from_memory(&collector.frames[0].data).expect("压缩预览帧必须能够解码");
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
    }
}
