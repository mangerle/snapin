//! 选区拖拽与尺寸调整几何计算。

use crate::snipper::selection::{DragMode, Rect, ResizeHandle};

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
        DragMode::Resize(handle) => {
            let cx = cursor.0;
            let cy = cursor.1;
            let new_rect = match handle {
                ResizeHandle::TopLeft => {
                    let anchor = (rect.x + rect.width, rect.y + rect.height);
                    Rect::from_points(anchor, (cx, cy))
                }
                ResizeHandle::Top => {
                    let anchor_y = rect.y + rect.height;
                    let y1 = cy.min(anchor_y);
                    let y2 = cy.max(anchor_y);
                    Rect {
                        x: rect.x,
                        y: y1,
                        width: rect.width,
                        height: (y2 - y1).max(1.0),
                    }
                }
                ResizeHandle::TopRight => {
                    let anchor = (rect.x, rect.y + rect.height);
                    Rect::from_points(anchor, (cx, cy))
                }
                ResizeHandle::Right => {
                    let x1 = rect.x.min(cx);
                    let x2 = rect.x.max(cx);
                    Rect {
                        x: x1,
                        y: rect.y,
                        width: (x2 - x1).max(1.0),
                        height: rect.height,
                    }
                }
                ResizeHandle::BottomRight => {
                    let anchor = (rect.x, rect.y);
                    Rect::from_points(anchor, (cx, cy))
                }
                ResizeHandle::Bottom => {
                    let y1 = rect.y.min(cy);
                    let y2 = rect.y.max(cy);
                    Rect {
                        x: rect.x,
                        y: y1,
                        width: rect.width,
                        height: (y2 - y1).max(1.0),
                    }
                }
                ResizeHandle::BottomLeft => {
                    let anchor = (rect.x + rect.width, rect.y);
                    Rect::from_points(anchor, (cx, cy))
                }
                ResizeHandle::Left => {
                    let anchor_x = rect.x + rect.width;
                    let x1 = cx.min(anchor_x);
                    let x2 = cx.max(anchor_x);
                    Rect {
                        x: x1,
                        y: rect.y,
                        width: (x2 - x1).max(1.0),
                        height: rect.height,
                    }
                }
            };
            (new_rect, DragMode::Resize(*handle))
        }
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

    #[test]
    fn test_calculate_drag_resize_handles() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 100.0,
        };

        // 1. 拖动右下角 (BottomRight)
        let (r_br, _) = calculate_drag(
            &rect,
            &DragMode::Resize(ResizeHandle::BottomRight),
            (350.0, 250.0),
        );
        assert_eq!(r_br.x, 100.0);
        assert_eq!(r_br.y, 100.0);
        assert_eq!(r_br.width, 250.0);
        assert_eq!(r_br.height, 150.0);

        // 2. 拖动左上角 (TopLeft)
        let (r_tl, _) = calculate_drag(
            &rect,
            &DragMode::Resize(ResizeHandle::TopLeft),
            (80.0, 70.0),
        );
        assert_eq!(r_tl.x, 80.0);
        assert_eq!(r_tl.y, 70.0);
        assert_eq!(r_tl.width, 220.0);
        assert_eq!(r_tl.height, 130.0);

        // 3. 拖动顶部 (Top)
        let (r_t, _) = calculate_drag(&rect, &DragMode::Resize(ResizeHandle::Top), (200.0, 60.0));
        assert_eq!(r_t.x, 100.0);
        assert_eq!(r_t.y, 60.0);
        assert_eq!(r_t.width, 200.0);
        assert_eq!(r_t.height, 140.0);

        // 4. 拖动右边 (Right)
        let (r_r, _) = calculate_drag(
            &rect,
            &DragMode::Resize(ResizeHandle::Right),
            (320.0, 150.0),
        );
        assert_eq!(r_r.x, 100.0);
        assert_eq!(r_r.y, 100.0);
        assert_eq!(r_r.width, 220.0);
        assert_eq!(r_r.height, 100.0);
    }
}
