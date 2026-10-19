//! 选区拖拽与尺寸调整几何计算。

use crate::snipper::selection::{DragMode, Rect};

/// 根据鼠标光标当前坐标与拖拽模式计算更新后的选区矩形
pub fn calculate_drag(rect: &Rect, drag: &DragMode, cursor: (f32, f32)) -> (Rect, DragMode) {
    match drag {
        DragMode::Move { offset_x, offset_y } => (
            Rect {
                x: cursor.0 - *offset_x as f32,
                y: cursor.1 - *offset_y as f32,
                ..*rect
            },
            DragMode::Move {
                offset_x: *offset_x,
                offset_y: *offset_y,
            },
        ),
        DragMode::Resize(h) => (
            Rect::from_points((rect.x, rect.y), cursor),
            DragMode::Resize(*h),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_drag_move() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 150.0,
        };
        let drag = DragMode::Move {
            offset_x: 20,
            offset_y: 30,
        };
        let (new_rect, _) = calculate_drag(&rect, &drag, (150.0, 160.0));
        assert_eq!(new_rect.x, 130.0);
        assert_eq!(new_rect.y, 130.0);
        assert_eq!(new_rect.width, 200.0);
        assert_eq!(new_rect.height, 150.0);
    }
}
