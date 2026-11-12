//! 长截图手动裁剪增量撤销历史栈与单轴条带还原辅助。

use super::types::GrowthPolarity;
use image::RgbaImage;
use std::collections::VecDeque;
use std::sync::Arc;

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
    /// 仅缓存被裁掉的窄边缘条带而非整张超长画布，并通过 `Arc` 共享避免克隆历史栈时重复拷贝像素。
    pub(crate) removed_strip: Arc<RgbaImage>,
    pub(crate) prev_last_frame: Arc<RgbaImage>,
    pub(crate) prev_anchor_truncated_below_frame: bool,
    pub(crate) prev_attached_header_size: u32,
    pub(crate) prev_attached_footer_size: u32,
    pub(crate) prev_growth_polarity: Option<GrowthPolarity>,
    /// 该裁剪记录之后对侧边缘是否已继续拼接推进；为 true 时撤回仅恢复本侧边缘像素与对应固定栏厚度。
    pub(crate) opposite_edge_after_stitch: bool,
}

/// 有界手动裁剪撤回栈。
#[derive(Debug, Clone, Default)]
pub(crate) struct CropHistory {
    entries: VecDeque<CropHistoryEntry>,
}

impl CropHistory {
    pub(crate) fn push(&mut self, entry: CropHistoryEntry) {
        if self.entries.len() >= MAX_CROP_HISTORY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub(crate) fn pop(&mut self) -> Option<CropHistoryEntry> {
        self.entries.pop_back()
    }

    /// 当画布某一端因新帧拼接或回滚推进而被修改时，仅失效同侧记录并保留对侧可无损撤销的裁剪历史。
    pub(crate) fn retain_opposite_edge(&mut self, mutated_edge: CropEdge) {
        self.entries.retain(|entry| entry.edge != mutated_edge);
        for entry in &mut self.entries {
            entry.opposite_edge_after_stitch = true;
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crop_history_evicts_oldest_in_constant_time_and_preserves_lifo() {
        let mut history = CropHistory::default();
        for i in 0..=(MAX_CROP_HISTORY + 2) {
            history.push(CropHistoryEntry {
                edge: CropEdge::Tail,
                removed_strip: Arc::new(RgbaImage::new(1, 1)),
                prev_last_frame: Arc::new(RgbaImage::new(1, 1)),
                prev_anchor_truncated_below_frame: false,
                prev_attached_header_size: i as u32,
                prev_attached_footer_size: 0,
                prev_growth_polarity: None,
                opposite_edge_after_stitch: false,
            });
        }

        let mut popped = Vec::with_capacity(MAX_CROP_HISTORY);
        while let Some(entry) = history.pop() {
            popped.push(entry.prev_attached_header_size);
        }
        assert_eq!(popped.len(), MAX_CROP_HISTORY);
        assert_eq!(popped.first().copied(), Some((MAX_CROP_HISTORY + 2) as u32));
        assert_eq!(popped.last().copied(), Some(3));
        assert!(history.is_empty());
    }
}
