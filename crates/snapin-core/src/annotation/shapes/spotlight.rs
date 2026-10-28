//! 聚光灯遮罩图元。
//!
//! 将聚光灯矩形之外的区域整体压暗，突出矩形内的内容；在撤销/重做时按底图
//! 重新绘制，不保存像素快照。

use super::{resize_rect, valid_rect};
use crate::annotation::action::{AnnotationAction, ShapeRect};
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PixmapMut, Rect as SkiaRect, Transform};

/// 聚光灯动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpotlightAction {
    pub rect: (f32, f32, f32, f32),
}

impl AnnotationAction for SpotlightAction {
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (rx, ry, rw, rh) = self.rect;
        let width = pixmap.width() as f32;
        let height = pixmap.height() as f32;
        let left = (rx + offset_x).max(0.0).min(width);
        let top = (ry + offset_y).max(0.0).min(height);
        let right = (rx + rw + offset_x).max(0.0).min(width);
        let bottom = (ry + rh + offset_y).max(0.0).min(height);
        if right <= left || bottom <= top {
            return;
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, 160);
        let areas = [
            (0.0, 0.0, width, top),
            (0.0, bottom, width, height - bottom),
            (0.0, top, left, bottom - top),
            (right, top, width - right, bottom - top),
        ];
        for (x, y, w, h) in areas {
            if let Some(rect) = SkiaRect::from_xywh(x, y, w, h) {
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
    }

    fn bounds(&self) -> Option<ShapeRect> {
        valid_rect(self.rect)
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.rect.0 += dx;
        self.rect.1 += dy;
    }

    fn resize_from_bounds(&mut self, from: ShapeRect, to: ShapeRect) {
        self.rect = resize_rect(from, to, self.rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    #[test]
    fn test_spotlight_dims_outside_and_keeps_focus() {
        let mut pixmap = Pixmap::new(9, 9).expect("创建测试位图失败");
        pixmap.fill(Color::WHITE);
        let action = SpotlightAction {
            rect: (3.0, 3.0, 3.0, 3.0),
        };
        action.draw(&mut pixmap.as_mut());

        let inside = &pixmap.data()[((4 * 9) + 4) * 4..((4 * 9) + 4) * 4 + 4];
        assert_eq!(inside, &[255, 255, 255, 255]);
        let outside = &pixmap.data()[0..4];
        assert!(outside[0] < 255, "聚光灯外区域应被压暗");
        assert_eq!(outside[3], 255, "压暗层应保持不透明");
    }
}
