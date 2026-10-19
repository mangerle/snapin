//! 椭圆与圆形标注图元绘制实现。

use crate::annotation::action::AnnotationAction;
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Rect as SkiaRect, Stroke, Transform};

/// 椭圆与圆形绘制动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EllipseAction {
    pub rect: (f32, f32, f32, f32),
    pub color: (u8, u8, u8, u8),
    pub stroke_width: f32,
    pub is_filled: bool,
}

impl AnnotationAction for EllipseAction {
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (x, y, w, h) = self.rect;
        if let Some(r) = SkiaRect::from_xywh(x + offset_x, y + offset_y, w, h) {
            let mut paint = Paint::default();
            paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

            let mut pb = PathBuilder::new();
            pb.push_oval(r);
            if let Some(path) = pb.finish() {
                if self.is_filled {
                    pixmap.fill_path(
                        &path,
                        &paint,
                        tiny_skia::FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                } else {
                    let stroke = Stroke {
                        width: self.stroke_width,
                        ..Default::default()
                    };
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }
        }
    }

    fn adjust_size(&mut self, stroke_w: f32, _font_sz: f32) {
        self.stroke_width = stroke_w;
    }
}
