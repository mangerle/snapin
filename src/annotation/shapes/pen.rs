//! 平滑自由画笔标注图元实现。

use crate::annotation::action::AnnotationAction;
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 普通涂鸦画笔动作
pub struct PenAction {
    pub points: Vec<(f32, f32)>,
    pub color: (u8, u8, u8, u8),
    pub stroke_width: f32,
}

impl AnnotationAction for PenAction {
    fn draw(&self, pixmap: &mut PixmapMut) {
        if self.points.len() < 2 {
            return;
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

        let stroke = Stroke {
            width: self.stroke_width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };

        let mut pb = PathBuilder::new();
        pb.move_to(self.points[0].0, self.points[0].1);
        for pt in &self.points[1..] {
            pb.line_to(pt.0, pt.1);
        }

        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}
