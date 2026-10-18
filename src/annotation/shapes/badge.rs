//! 步骤数字序号气泡标号图元绘制实现。

use crate::annotation::action::AnnotationAction;
use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, FontSystem, Metrics, Shaping, SwashCache, Weight,
};
use std::cell::RefCell;
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Stroke, Transform};

thread_local! {
    static FONT_SYSTEM: RefCell<(FontSystem, SwashCache)> = RefCell::new((
        FontSystem::new(),
        SwashCache::new(),
    ));
}

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

        // 3. 使用 cosmic-text 高精排版渲染现代纯白加粗数字 (绝对光学居中)
        draw_badge_digit(pixmap, cx, cy, self.number, r);
    }

    fn adjust_size(&mut self, stroke_w: f32, _font_sz: f32) {
        self.radius = (stroke_w * 3.0).clamp(10.0, 48.0);
    }
}

/// 在气泡中心高精排版绘制加粗现代无衬线数字
fn draw_badge_digit(pixmap: &mut PixmapMut, cx: f32, cy: f32, num: usize, r: f32) {
    let text = num.to_string();
    let font_size = if num < 10 {
        r * 1.35
    } else if num < 100 {
        r * 1.05
    } else {
        r * 0.8
    };

    FONT_SYSTEM.with(|cell| {
        let mut borrowed = cell.borrow_mut();
        let (font_system, swash_cache) = &mut *borrowed;

        let metrics = Metrics::new(font_size, font_size * 1.25);
        let mut buffer = Buffer::new(font_system, metrics);
        buffer.set_size(font_system, r * 4.0, r * 4.0);
        buffer.set_text(
            font_system,
            &text,
            Attrs::new().weight(Weight::BOLD),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(font_system, false);

        // 先计算所有 glyph 的实际像素包围盒，确保精准几何光学居中
        let mut min_gx = f32::MAX;
        let mut max_gx = f32::MIN;
        let mut min_gy = f32::MAX;
        let mut max_gy = f32::MIN;
        let mut has_glyphs = false;

        buffer.draw(
            font_system,
            swash_cache,
            CosmicColor::rgba(255, 255, 255, 255),
            |gx, gy, w, h, _c| {
                if w > 0 && h > 0 {
                    has_glyphs = true;
                    min_gx = min_gx.min(gx as f32);
                    max_gx = max_gx.max((gx + w as i32) as f32);
                    min_gy = min_gy.min(gy as f32);
                    max_gy = max_gy.max((gy + h as i32) as f32);
                }
            },
        );

        let (offset_x, offset_y) = if has_glyphs {
            let text_center_x = (min_gx + max_gx) / 2.0;
            let text_center_y = (min_gy + max_gy) / 2.0;
            (cx - text_center_x, cy - text_center_y)
        } else {
            (cx - font_size * 0.3, cy - font_size * 0.5)
        };

        let width = pixmap.width() as i32;
        let height = pixmap.height() as i32;

        buffer.draw(
            font_system,
            swash_cache,
            CosmicColor::rgba(255, 255, 255, 255),
            |gx, gy, _w, _h, c| {
                let px = offset_x as i32 + gx;
                let py = offset_y as i32 + gy;
                if px >= 0 && px < width && py >= 0 && py < height {
                    let idx = ((py as usize) * (width as usize) + (px as usize)) * 4;
                    let data = pixmap.data_mut();
                    let src_a = c.a() as f32 / 255.0;
                    let dst_a = data[idx + 3] as f32 / 255.0;
                    let out_a = src_a + dst_a * (1.0 - src_a);
                    if out_a > 0.0 {
                        let src_r = c.r() as f32 * src_a;
                        let src_g = c.g() as f32 * src_a;
                        let src_b = c.b() as f32 * src_a;
                        let dst_r = data[idx] as f32 * dst_a;
                        let dst_g = data[idx + 1] as f32 * dst_a;
                        let dst_b = data[idx + 2] as f32 * dst_a;
                        data[idx] = ((src_r + dst_r * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 1] = ((src_g + dst_g * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 2] = ((src_b + dst_b * (1.0 - src_a)) / out_a) as u8;
                        data[idx + 3] = (out_a * 255.0) as u8;
                    }
                }
            },
        );
    });
}
