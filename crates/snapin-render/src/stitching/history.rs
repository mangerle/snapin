//! 长截图手动裁剪增量撤销历史栈与单轴条带还原辅助。

use super::types::{GrowthPolarity, StitchDirection};
use image::{GenericImage, RgbaImage};

/// 单次会话最多保留的手动裁剪撤销步数，防止频繁重录或连续裁剪长期占用堆内存。
const MAX_CROP_HISTORY: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CropEdge {
    Head,
    Tail,
}

#[derive(Debug, Clone)]
pub(crate) struct CropHistoryEntry {
    pub(crate) edge: CropEdge,
    /// 仅缓存被裁掉的窄边缘条带而非整张超长画布，兼顾多步撤回与低内存占用。
    pub(crate) removed_strip: RgbaImage,
    pub(crate) prev_last_frame: RgbaImage,
    pub(crate) prev_anchor_truncated_below_frame: bool,
    pub(crate) prev_attached_header_size: u32,
    pub(crate) prev_attached_footer_size: u32,
    pub(crate) prev_growth_polarity: Option<GrowthPolarity>,
}

/// 有界手动裁剪撤回栈。
#[derive(Debug, Clone, Default)]
pub(crate) struct CropHistory {
    entries: Vec<CropHistoryEntry>,
}

impl CropHistory {
    pub(crate) fn push(&mut self, entry: CropHistoryEntry) {
        if self.entries.len() >= MAX_CROP_HISTORY {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    pub(crate) fn pop(&mut self) -> Option<CropHistoryEntry> {
        self.entries.pop()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 沿主拼接轴将前后两幅等副轴位图无损拼接为一幅画布，用于撤销头部或尾部手动裁剪。
pub(crate) fn concat_along_axis(
    first: &RgbaImage,
    second: &RgbaImage,
    direction: StitchDirection,
) -> Option<RgbaImage> {
    let (w1, h1) = first.dimensions();
    let (w2, h2) = second.dimensions();
    let mut combined = match direction {
        StitchDirection::Vertical => RgbaImage::new(w1.max(w2), h1.saturating_add(h2)),
        StitchDirection::Horizontal => RgbaImage::new(w1.saturating_add(w2), h1.max(h2)),
    };
    combined.copy_from(first, 0, 0).ok()?;
    let (dx, dy) = match direction {
        StitchDirection::Vertical => (0, h1),
        StitchDirection::Horizontal => (w1, 0),
    };
    combined.copy_from(second, dx, dy).ok()?;
    Some(combined)
}
