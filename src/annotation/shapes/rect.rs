//! 矩形标注图元绘制实现。

use crate::annotation::action::AnnotationAction;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Rect as SkiaRect, Stroke, Transform};

/// 矩形绘制动作
pub struct RectAction {
    pub rect: (f32, f32, f32, f32),
    pub color: (u8, u8, u8, u8),
    pub stroke_width: f32,
    pub is_filled: bool,
}

impl AnnotationAction for RectAction {
    fn draw(&self, pixmap: &mut PixmapMut) {
        let (x, y, w, h) = self.rect;
        if let Some(r) = SkiaRect::from_xywh(x, y, w, h) {
            let mut paint = Paint::default();
            paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

            if self.is_filled {
                pixmap.fill_rect(r, &paint, Transform::identity(), None);
            } else {
                let stroke = Stroke {
                    width: self.stroke_width,
                    ..Default::default()
                };
                let mut pb = PathBuilder::new();
                pb.push_rect(r);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }
        }
    }
}
