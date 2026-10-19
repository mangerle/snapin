//! 截图原位翻译替换图元光栅化实现。

use crate::annotation::action::AnnotationAction;
use crate::render::with_thread_local_text_renderer;
use serde::{Deserialize, Serialize};
use tiny_skia::{Color, Paint, PathBuilder, PixmapMut, Transform};

/// 单个原位翻译文本项
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranslationShapeItem {
    /// 选区内坐标 [x, y, width, height]
    pub rect: (f32, f32, f32, f32),
    /// 原文
    pub original: String,
    /// 译文
    pub translated: String,
    /// 擦除背景主导色 RGBA
    pub bg_color: (u8, u8, u8, u8),
    /// 译文字体前景色 RGBA
    pub text_color: (u8, u8, u8, u8),
    /// 适配后的字号大小 (pt)
    pub font_size: f32,
    /// 覆盖模式: "solid_color" | "frosted_glass"
    pub cover_mode: String,
}

/// 原图原位翻译复合动作图元
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranslationAction {
    pub items: Vec<TranslationShapeItem>,
}

impl TranslationAction {
    pub fn new(items: Vec<TranslationShapeItem>) -> Self {
        Self { items }
    }
}

impl AnnotationAction for TranslationAction {
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        if self.items.is_empty() {
            return;
        }

        // 1. 批量绘制背景擦除层 (覆盖遮蔽原文字)
        for item in &self.items {
            let rx = item.rect.0 + offset_x;
            let ry = item.rect.1 + offset_y;
            let rw = item.rect.2;
            let rh = item.rect.3;

            if rw <= 1.0 || rh <= 1.0 {
                continue;
            }

            // 根据覆盖模式决定擦除层 Alpha
            let alpha = if item.cover_mode == "frosted_glass" {
                180 // 半透明磨砂遮罩 (约 70% 不透明度)
            } else {
                255 // 智能实色平铺擦除
            };

            let bg_c = Color::from_rgba8(item.bg_color.0, item.bg_color.1, item.bg_color.2, alpha);

            let mut paint = Paint::default();
            paint.set_color(bg_c);
            paint.anti_alias = true;

            // 绘制平滑微圆角擦除矩形 (平滑过渡边缘)
            let mut pb = PathBuilder::new();
            let radius = 3.0_f32.min(rw / 2.0).min(rh / 2.0);
            pb.move_to(rx + radius, ry);
            pb.line_to(rx + rw - radius, ry);
            pb.quad_to(rx + rw, ry, rx + rw, ry + radius);
            pb.line_to(rx + rw, ry + rh - radius);
            pb.quad_to(rx + rw, ry + rh, rx + rw - radius, ry + rh);
            pb.line_to(rx + radius, ry + rh);
            pb.quad_to(rx, ry + rh, rx, ry + rh - radius);
            pb.line_to(rx, ry + radius);
            pb.quad_to(rx, ry, rx + radius, ry);
            pb.close();

            if let Some(path) = pb.finish() {
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        // 2. 批量调用 cosmic-text 文本渲染器绘制译文
        use crate::render::TextBoxOptions;
        with_thread_local_text_renderer(|renderer| {
            for item in &self.items {
                if item.translated.trim().is_empty() {
                    continue;
                }

                let rx = item.rect.0 + offset_x + 2.0;
                let ry = item.rect.1 + offset_y + 2.0;
                let rw = (item.rect.2 - 4.0).max(10.0);
                let rh = (item.rect.3 - 4.0).max(10.0);
                let font_size = item.font_size.max(9.0);

                renderer.draw_text_box_rgba(
                    pixmap,
                    &item.translated,
                    TextBoxOptions {
                        rect: (rx, ry, rw, rh),
                        font_size,
                        rgba: item.text_color,
                    },
                );
            }
        });
    }

    fn adjust_size(&mut self, _stroke_w: f32, font_sz: f32) {
        for item in &mut self.items {
            item.font_size = font_sz;
        }
    }
}
