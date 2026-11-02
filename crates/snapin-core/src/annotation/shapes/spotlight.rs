//! 聚光灯遮罩图元。
//!
//! 将聚光灯矩形之外的区域整体压暗，突出矩形内的内容；在撤销/重做时按底图
//! 重新绘制，不保存像素快照。

use super::{rect_from_tuple, resize_rect, tuple_from_rect, valid_rect, SizeParam};
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PixmapMut, Rect as SkiaRect, Transform};

/// 聚光灯动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpotlightAction {
    pub rect: (f32, f32, f32, f32),
    /// 聚光灯外区域压暗强度（0 到 255）
    #[serde(default = "default_intensity")]
    pub intensity: u8,
}

fn default_intensity() -> u8 {
    160
}

impl SpotlightAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
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
        paint.set_color_rgba8(0, 0, 0, self.intensity);
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

    pub(crate) fn adjust_size(&mut self, _param: SizeParam) {}

    pub(crate) fn bounds(&self) -> Option<Rect> {
        valid_rect(rect_from_tuple(self.rect))
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.rect.0 += dx;
        self.rect.1 += dy;
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.rect = tuple_from_rect(resize_rect(from, to, rect_from_tuple(self.rect)));
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl SpotlightAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
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
            intensity: 160,
        };
        action.draw(&mut pixmap.as_mut());

        let inside = &pixmap.data()[((4 * 9) + 4) * 4..((4 * 9) + 4) * 4 + 4];
        assert_eq!(inside, &[255, 255, 255, 255]);
        let outside = &pixmap.data()[0..4];
        assert!(outside[0] < 255, "聚光灯外区域应被压暗");
        assert_eq!(outside[3], 255, "压暗层应保持不透明");
    }
}
