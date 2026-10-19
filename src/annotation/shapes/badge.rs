//! 步骤数字序号气泡标号图元绘制实现。

use crate::annotation::action::AnnotationAction;
use crate::render::with_thread_local_text_renderer;
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 步骤序号标号动作
pub struct BadgeAction {
    pub center: (f32, f32),
    pub number: usize,
    pub color: (u8, u8, u8, u8),
    pub radius: f32,
}

impl AnnotationAction for BadgeAction {
    fn draw(&self, pixmap: &mut PixmapMut) {
        let (cx, cy) = self.center;
        let r = self.radius;

        // 1. 绘制气泡柔和外圈微阴影
        let mut pb_shadow = PathBuilder::new();
        pb_shadow.push_circle(cx, cy + 1.0, r + 1.0);
        if let Some(path) = pb_shadow.finish() {
            let mut shadow_paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            shadow_paint.set_color(Color::from_rgba8(0, 0, 0, 45));
            pixmap.fill_path(
                &path,
                &shadow_paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        // 2. 绘制圆形彩色背景底板
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, r);
        if let Some(path) = pb.finish() {
            let mut paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );

            // 白色现代精致外圆环 (高对比度描边)
            let stroke_paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            let stroke = Stroke {
                width: 1.8,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
        }

        // 3. 使用统一文本基础设施高精排版渲染现代纯白加粗数字 (绝对光学居中)
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_badge_digit(pixmap, cx, cy, self.number, r);
        });
    }

    fn adjust_size(&mut self, stroke_w: f32, _font_sz: f32) {
        self.radius = (stroke_w * 3.0).clamp(10.0, 48.0);
    }
}
