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

/// 选区尺寸约束。
///
/// 使用携带尺寸数据的枚举而不是多个布尔开关，确保“自由、固定比例、固定尺寸”
/// 三种互斥状态在类型层面不可同时存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionConstraint {
    /// 不限制宽高
    #[default]
    Free,
    /// 固定宽高比
    Ratio {
        /// 横向比例
        width: u32,
        /// 纵向比例
        height: u32,
    },
    /// 固定像素尺寸
    Fixed {
        /// 宽度（物理像素）
        width: u32,
        /// 高度（物理像素）
        height: u32,
    },
}

impl SelectionConstraint {
    /// 状态栏展示名称。
    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "自由",
            Self::Ratio {
                width: 1,
                height: 1,
            } => "1:1",
            Self::Ratio {
                width: 4,
                height: 3,
            } => "4:3",
            Self::Ratio {
                width: 16,
                height: 9,
            } => "16:9",
            Self::Ratio { .. } => "固定比例",
            Self::Fixed { width, height } => match (width, height) {
                (800, 600) => "800x600",
                (1280, 720) => "1280x720",
                _ => "固定尺寸",
            },
        }
    }

    /// 按约束修正拖拽终点，保持起点不动。
    pub fn constrain(self, start: (f32, f32), current: (f32, f32)) -> (f32, f32) {
        let dx = current.0 - start.0;
        let dy = current.1 - start.1;
        match self {
            Self::Free => current,
            Self::Ratio { width, height } if width > 0 && height > 0 => {
                let ratio = width as f32 / height as f32;
                let raw_width = dx.abs().max(dy.abs() * ratio);
                let width = raw_width.max(1.0);
                let height = (width / ratio).max(1.0);
                (
                    start.0 + width * signed_direction(dx),
                    start.1 + height * signed_direction(dy),
                )
            }
            Self::Fixed { width, height } if width > 0 && height > 0 => (
                start.0 + width as f32 * signed_direction(dx),
                start.1 + height as f32 * signed_direction(dy),
            ),
            _ => current,
        }
    }
}

/// 零位移使用正方向，避免固定尺寸选区在首次按下时反向跳变。
fn signed_direction(value: f32) -> f32 {
    if value < 0.0 {
        -1.0
    } else {
        1.0
    }
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

    /// 固定比例必须保持宽高比并跟随拖拽方向。
    #[test]
    fn test_ratio_constraint_preserves_direction() {
        let constrained = SelectionConstraint::Ratio {
            width: 16,
            height: 9,
        }
        .constrain((10.0, 20.0), (90.0, 50.0));
        let width = constrained.0 - 10.0;
        let height = constrained.1 - 20.0;
        assert!((width / height - 16.0 / 9.0).abs() < 0.001);

        let reverse = SelectionConstraint::Ratio {
            width: 1,
            height: 1,
        }
        .constrain((100.0, 100.0), (80.0, 120.0));
        assert!(reverse.0 < 100.0 && reverse.1 > 100.0);
        assert!((reverse.0 - 80.0).abs() < 0.001);
    }

    /// 固定尺寸直接使用配置像素值。
    #[test]
    fn test_fixed_constraint_uses_exact_size() {
        let constrained = SelectionConstraint::Fixed {
            width: 800,
            height: 600,
        }
        .constrain((10.0, 20.0), (90.0, 50.0));
        assert_eq!(constrained, (810.0, 620.0));
    }
}
