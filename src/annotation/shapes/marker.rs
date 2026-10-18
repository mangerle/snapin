//! 荧光记号笔标注图元实现。
//!
//! 采用半透明或正片叠底（Multiply）混合模式，确保高亮文本时底部的黑色文字永不被遮蔽。

use crate::annotation::action::AnnotationAction;
use tiny_skia::{BlendMode, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 荧光记号笔动作
pub struct MarkerAction {
    pub points: Vec<(f32, f32)>,
    pub color: (u8, u8, u8),
    pub stroke_width: f32,
}

impl AnnotationAction for MarkerAction {
    fn draw(&self, pixmap: &mut PixmapMut) {
        if self.points.len() < 2 {
            return;
        }

        let mut paint = Paint::default();
        // 50% 半透明荧光色，搭配 Multiply 混合模式
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, 110);
        paint.blend_mode = BlendMode::Multiply;

        let stroke = Stroke {
            width: self.stroke_width,
            line_cap: LineCap::Square,
            line_join: LineJoin::Miter,
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
