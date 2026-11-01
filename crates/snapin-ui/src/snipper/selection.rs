//! 鼠标选区状态机与手柄交互控制。
//!
//! 支持鼠标拖拽选区、8 个锚点微调缩放、选区平移以及按逗号 `,` 和句号 `.` 快速回溯历史选区。

pub use crate::geometry::{hit_test_handle, Rect, ResizeHandle};

/// 选区交互状态机
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SelectionState {
    /// 未建立选区，光标自由移动
    #[default]
    Idle,
    /// 鼠标左键按住拖拽创建初始选区
    Creating {
        start: (f32, f32),
        current: (f32, f32),
    },
    /// 选区已锁定，处于静止或微调状态
    Locked {
        rect: Rect,
        active_drag: Option<DragMode>,
    },
}

/// 拖拽微调模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    /// 整体移动选区
    Move { offset_x: i32, offset_y: i32 },
    /// 拖动某个锚点缩放
    Resize(ResizeHandle),
}

/// 历史选区回溯队列
#[derive(Debug, Default, Clone)]
pub struct SelectionHistory {
    records: Vec<Rect>,
    cursor: usize,
}

impl SelectionHistory {
    /// 压入一条新完成的有效选区
    pub fn push(&mut self, rect: Rect) {
        if rect.width > 10.0 && rect.height > 10.0 {
            // 最多保存 10 条历史
            if self.records.len() >= 10 {
                self.records.remove(0);
            }
            self.records.push(rect);
            self.cursor = self.records.len().saturating_sub(1);
        }
    }

    /// 按逗号键 `,` 回退至上一条历史选区
    pub fn previous_history(&mut self) -> Option<Rect> {
        if self.records.is_empty() {
            return None;
        }
        let res = self.records.get(self.cursor).copied();
        self.cursor = self.cursor.saturating_sub(1);
        res
    }

    /// 按句号键 `.` 前进到下一条历史选区
    pub fn next_history(&mut self) -> Option<Rect> {
        if self.records.is_empty() {
            return None;
        }
        if self.cursor + 1 < self.records.len() {
            self.cursor += 1;
        }
        self.records.get(self.cursor).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_history() {
        let mut history = SelectionHistory::default();
        let r1 = Rect::from_points((0.0, 0.0), (100.0, 100.0));
        let r2 = Rect::from_points((10.0, 10.0), (80.0, 80.0));
        history.push(r1);
        history.push(r2);

        assert_eq!(history.previous_history(), Some(r2));
        assert_eq!(history.previous_history(), Some(r1));
        assert_eq!(history.next_history(), Some(r2));
    }
}
