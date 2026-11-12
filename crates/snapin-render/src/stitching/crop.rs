//! 长截图手动首/尾裁剪、增量历史栈维护与可逆撤销扩展。

use super::history::{CropEdge, CropHistoryEntry};
use super::stitcher::LongCaptureStitcher;
use super::types::{GrowthPolarity, StitchDirection, MIN_CROPPED_SPAN};
use image::RgbaImage;
use std::sync::Arc;

impl LongCaptureStitcher {
    /// 手动裁切画布首部（上裁剪 / 左裁剪），返回实际裁去的像素长度。
    pub fn crop_head(&mut self, pixels: u32) -> u32 {
        let crop = self.clamp_manual_crop(pixels);
        let Some(removed_strip) = self.canvas.pop_head(crop) else {
            return 0;
        };
        self.push_crop_history(CropEdge::Head, removed_strip);
        self.attached_header_size = self.attached_header_size.saturating_sub(crop);
        self.refresh_polarity_and_anchor_after_crop();
        self.generation = self.generation.wrapping_add(1);
        crop
    }

    /// 手动裁切画布尾部（下裁剪 / 右裁剪），返回实际裁去的像素长度。
    pub fn crop_tail(&mut self, pixels: u32) -> u32 {
        let crop = self.clamp_manual_crop(pixels);
        let Some(removed_strip) = self.canvas.pop_tail(crop) else {
            return 0;
        };
        self.push_crop_history(CropEdge::Tail, removed_strip);
        self.attached_footer_size = self.attached_footer_size.saturating_sub(crop);
        self.refresh_polarity_and_anchor_after_crop();
        self.generation = self.generation.wrapping_add(1);
        crop
    }

    fn push_crop_history(&mut self, edge: CropEdge, removed_strip: RgbaImage) {
        self.crop_history.push(CropHistoryEntry {
            edge,
            removed_strip: Arc::new(removed_strip),
            prev_last_frame: self.last_frame.clone(),
            prev_anchor_truncated_below_frame: self.anchor_truncated_below_frame,
            prev_attached_header_size: self.attached_header_size,
            prev_attached_footer_size: self.attached_footer_size,
            prev_growth_polarity: self.growth_polarity,
        });
    }

    /// 当前是否存在可撤销的手动头/尾裁剪记录。
    pub fn can_undo_crop(&self) -> bool {
        !self.crop_history.is_empty()
    }

    /// 当前画布是否仍有继续执行手动头/尾裁剪的余量（未触及最小保护跨度）。
    pub fn can_crop(&self) -> bool {
        self.clamp_manual_crop(1) > 0
    }

    /// 激活对齐端是否已被手动裁短至单帧尺寸以下（此时禁止继续直接拼接后续帧）。
    pub const fn is_anchor_truncated_below_frame(&self) -> bool {
        self.anchor_truncated_below_frame
    }

    /// 兼容既有命名：等价于 [`Self::is_anchor_truncated_below_frame`]。
    pub const fn is_tail_truncated_below_frame(&self) -> bool {
        self.anchor_truncated_below_frame
    }

    /// 撤销最近一次手动头/尾裁剪并无损恢复画布、极性与锚点基准帧。
    pub fn undo_crop(&mut self) -> bool {
        let Some(entry) = self.crop_history.pop() else {
            return false;
        };
        match entry.edge {
            CropEdge::Head => self.canvas.prepend_contiguous(&entry.removed_strip),
            CropEdge::Tail => self.canvas.append_contiguous(&entry.removed_strip),
        }
        self.last_frame = entry.prev_last_frame;
        self.anchor_truncated_below_frame = entry.prev_anchor_truncated_below_frame;
        self.attached_header_size = entry.prev_attached_header_size;
        self.attached_footer_size = entry.prev_attached_footer_size;
        self.growth_polarity = entry.prev_growth_polarity;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    fn clamp_manual_crop(&self, pixels: u32) -> u32 {
        let (span, frame_span) = self.main_axis_spans();
        let min_span = MIN_CROPPED_SPAN.min(frame_span).min(span);
        pixels.min(span.saturating_sub(min_span))
    }

    pub(crate) fn main_axis_spans(&self) -> (u32, u32) {
        let frame_main = match self.direction {
            StitchDirection::Vertical => self.initial_frame_size.1,
            StitchDirection::Horizontal => self.initial_frame_size.0,
        };
        (self.canvas.main_span(), frame_main)
    }

    fn refresh_polarity_and_anchor_after_crop(&mut self) {
        let (span, frame_span) = self.main_axis_spans();
        if span <= frame_span {
            self.growth_polarity = None;
        }
        if span < frame_span || frame_span == 0 {
            self.anchor_truncated_below_frame = true;
            return;
        }
        let anchor_start = if self.growth_polarity == Some(GrowthPolarity::Backward) {
            0
        } else {
            span.saturating_sub(frame_span)
        };
        self.last_frame = Arc::new(self.canvas.slice_axis(anchor_start, frame_span));
        self.anchor_truncated_below_frame = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stitching::types::StitchOptions;

    #[test]
    fn test_can_crop_reports_false_for_tiny_initial_selection() {
        let tiny = RgbaImage::new(60, 24);
        let mut stitcher =
            LongCaptureStitcher::new(tiny, StitchDirection::Vertical, StitchOptions::default());
        assert!(!stitcher.can_crop());
        assert_eq!(stitcher.crop_head(30), 0);
        assert_eq!(stitcher.crop_tail(30), 0);
    }
}
