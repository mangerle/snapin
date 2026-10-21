//! 步骤数字序号气泡标号图元绘制实现。

use crate::annotation::action::AnnotationAction;
use crate::annotation::style::BadgeStyle;
use crate::render::with_thread_local_text_renderer;
use serde::{Deserialize, Serialize};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 步骤序号标号动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BadgeAction {
    pub center: (f32, f32),
    pub number: usize,
    pub color: (u8, u8, u8, u8),
    pub radius: f32,
    /// 底板样式：实心圆底或空心描边
    #[serde(default)]
    pub style: BadgeStyle,
}

impl AnnotationAction for BadgeAction {
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (cx, cy) = (self.center.0 + offset_x, self.center.1 + offset_y);
        let r = self.radius;

        if self.style == BadgeStyle::Outline {
            self.draw_outline(pixmap, cx, cy, r);
            return;
        }

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

impl BadgeAction {
    /// 空心描边样式：只画彩色圆环与同色数字，不遮挡底图
    fn draw_outline(&self, pixmap: &mut PixmapMut, cx: f32, cy: f32, r: f32) {
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, r);
        if let Some(path) = pb.finish() {
            let stroke = Stroke {
                width: (r * 0.16).clamp(1.8, 6.0),
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }

        with_thread_local_text_renderer(|renderer| {
            renderer.draw_badge_digit_rgba(pixmap, cx, cy, self.number, r, self.color);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    fn draw_badge(style: BadgeStyle) -> Pixmap {
        let mut pixmap = Pixmap::new(120, 120).expect("分配 Pixmap 成功");
        BadgeAction {
            center: (60.0, 60.0),
            number: 1,
            color: (220, 38, 38, 255),
            radius: 24.0,
            style,
        }
        .draw(&mut pixmap.as_mut());
        pixmap
    }

    /// 空心样式只保留圆环与数字，圆内非描边区域必须保持透明，不遮挡底图
    #[test]
    fn test_badge_outline_keeps_interior_transparent() {
        let filled = draw_badge(BadgeStyle::Filled);
        let outline = draw_badge(BadgeStyle::Outline);

        let alpha_at =
            |pixmap: &Pixmap, x: u32, y: u32| pixmap.pixel(x, y).map(|p| p.alpha()).unwrap_or(0);

        // 圆心右侧 15px 处：实心样式为纯色底，空心样式应透明（数字“1”笔画集中在中心列）
        assert_eq!(alpha_at(&filled, 75, 60), 255, "实心徽标内部应着色");
        assert_eq!(alpha_at(&outline, 75, 60), 0, "空心徽标内部应透明");

        // 圆环本身仍需着色
        assert!(alpha_at(&outline, 37, 60) > 0, "空心徽标圆环应描边");
    }
}
