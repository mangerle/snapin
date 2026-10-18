//! 步骤数字序号气泡标号图元绘制实现。

use crate::annotation::action::AnnotationAction;
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

        // 1. 绘制圆形彩色背景底板
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, r);
        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );

            // 白色细外圈
            let stroke_paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            let stroke = Stroke {
                width: 1.5,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);
        }

        // 2. 绘制内部白色数字
        draw_badge_digit(pixmap, cx, cy, self.number, r * 0.7);
    }
}

/// 绘制数字简笔矢量线条
fn draw_badge_digit(pixmap: &mut PixmapMut, cx: f32, cy: f32, num: usize, h: f32) {
    let mut pb = PathBuilder::new();
    let top = cy - h / 2.0;
    let bot = cy + h / 2.0;
    let left = cx - h / 3.0;
    let right = cx + h / 3.0;

    match num % 10 {
        1 => {
            pb.move_to(cx - 2.0, top + 3.0);
            pb.line_to(cx, top);
            pb.line_to(cx, bot);
        }
        2 => {
            pb.move_to(left, top + 2.0);
            pb.line_to(right, top + 2.0);
            pb.line_to(right, cy);
            pb.line_to(left, bot);
            pb.line_to(right, bot);
        }
        3 => {
            pb.move_to(left, top);
            pb.line_to(right, top);
            pb.line_to(right, cy);
            pb.line_to(left, cy);
            pb.move_to(right, cy);
            pb.line_to(right, bot);
            pb.line_to(left, bot);
        }
        4 => {
            pb.move_to(right, bot);
            pb.line_to(right, top);
            pb.line_to(left, cy);
            pb.line_to(right, cy);
        }
        5 => {
            pb.move_to(right, top);
            pb.line_to(left, top);
            pb.line_to(left, cy);
            pb.line_to(right, cy);
            pb.line_to(right, bot);
            pb.line_to(left, bot);
        }
        _ => {
            pb.move_to(left, top);
            pb.line_to(right, top);
            pb.line_to(right, bot);
            pb.line_to(left, bot);
            pb.close();
        }
    }

    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba8(255, 255, 255, 255));
        let stroke = Stroke {
            width: 2.0,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}
