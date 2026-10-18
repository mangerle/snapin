//! 鼠标选区状态机与手柄交互控制。
//!
//! 支持鼠标拖拽选区、8 个锚点微调缩放、选区平移以及按逗号 `,` 和句号 `.` 快速回溯历史选区。

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

/// 矩形几何描述 (x, y, width, height)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// 根据两个对角点构建正规化的矩形（确保宽高非负）
    pub fn from_points(p1: (f32, f32), p2: (f32, f32)) -> Self {
        let x = p1.0.min(p2.0);
        let y = p1.1.min(p2.1);
        let width = (p1.0 - p2.0).abs();
        let height = (p1.1 - p2.1).abs();
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 判断某个坐标点是否落在矩形内部
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    /// 整数像素对齐包围盒 (用于图像裁剪)
    pub fn to_pixel_bounds(&self, max_w: u32, max_h: u32) -> (u32, u32, u32, u32) {
        let px = (self.x.max(0.0) as u32).min(max_w);
        let py = (self.y.max(0.0) as u32).min(max_h);
        let pw = (self.width as u32).min(max_w.saturating_sub(px));
        let ph = (self.height as u32).min(max_h.saturating_sub(py));
        (px, py, pw, ph)
    }
}

/// 拖拽微调模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    /// 整体移动选区
    Move { offset_x: i32, offset_y: i32 },
    /// 拖动某个锚点缩放
    Resize(ResizeHandle),
}

/// 8 个边角控制手柄
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
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
    fn test_rect_normalization() {
        let rect = Rect::from_points((100.0, 200.0), (50.0, 80.0));
        assert_eq!(rect.x, 50.0);
        assert_eq!(rect.y, 80.0);
        assert_eq!(rect.width, 50.0);
        assert_eq!(rect.height, 120.0);
        assert!(rect.contains(60.0, 90.0));
        assert!(!rect.contains(10.0, 90.0));
    }

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
