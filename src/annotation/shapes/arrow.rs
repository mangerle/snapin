//! 指引箭头标注图元绘制实现。

use crate::annotation::action::AnnotationAction;
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 指引箭头绘制动作
pub struct ArrowAction {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub color: (u8, u8, u8, u8),
    pub stroke_width: f32,
}

impl AnnotationAction for ArrowAction {
    fn draw(&self, pixmap: &mut PixmapMut) {
        let (x1, y1) = self.start;
        let (x2, y2) = self.end;

        let dx = x2 - x1;
        let dy = y2 - y1;
        let length = (dx * dx + dy * dy).sqrt();
        if length < 1.0 {
            return;
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

        // 1. 绘制箭头主干线
        let stroke = Stroke {
            width: self.stroke_width,
            ..Default::default()
        };
        let mut pb = PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(x2, y2);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }

        // 2. 计算并绘制箭头三角形头标
        let arrow_size = (self.stroke_width * 3.5).max(12.0);
        let angle = dy.atan2(dx);
        let arrow_angle = std::f32::consts::PI / 6.0; // 30 度角

        let p_left_x = x2 - arrow_size * (angle - arrow_angle).cos();
        let p_left_y = y2 - arrow_size * (angle - arrow_angle).sin();
        let p_right_x = x2 - arrow_size * (angle + arrow_angle).cos();
        let p_right_y = y2 - arrow_size * (angle + arrow_angle).sin();

        let mut pb_head = PathBuilder::new();
        pb_head.move_to(x2, y2);
        pb_head.line_to(p_left_x, p_left_y);
        pb_head.line_to(p_right_x, p_right_y);
        pb_head.close();

        if let Some(head_path) = pb_head.finish() {
            pixmap.fill_path(
                &head_path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    fn adjust_size(&mut self, stroke_w: f32, _font_sz: f32) {
        self.stroke_width = stroke_w;
    }
}
