//! 截图原位翻译替换图元光栅化实现。

use super::{valid_rect, SizeParam};
use crate::config::enums::CoverMode;
use crate::geometry::Rect;
use crate::render::with_thread_local_text_renderer;
use crate::style::ColorRgba;
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
    pub bg_color: ColorRgba,
    /// 译文字体前景色 RGBA
    pub text_color: ColorRgba,
    /// 适配后的字号大小 (pt)
    pub font_size: f32,
    /// 覆盖模式
    pub cover_mode: CoverMode,
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

impl TranslationAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        if self.items.is_empty() {
            return;
        }

        // 1. 批量绘制背景擦除层 (覆盖遮蔽原文字)
        for item in &self.items {
            // 没有有效译文或译文与原文相同时不得擦除原图，否则会破坏截图内容。
            if item.translated.trim().is_empty() || item.translated.trim() == item.original.trim() {
                continue;
            }

            let rx = item.rect.0 + offset_x;
            let ry = item.rect.1 + offset_y;
            let rw = item.rect.2;
            let rh = item.rect.3;

            if rw <= 1.0 || rh <= 1.0 {
                continue;
            }

            // 根据覆盖模式决定擦除层 Alpha
            let alpha = match item.cover_mode {
                CoverMode::FrostedGlass => 180,
                CoverMode::SolidColor => 255,
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
                // 译文与原文相同时保留原图像素，避免无意义的覆盖与重新排版。
                if item.translated.trim().is_empty()
                    || item.translated.trim() == item.original.trim()
                {
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
                        rgba: item.text_color.to_tuple(),
                    },
                );
            }
        });
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Font(size) = param {
            for item in &mut self.items {
                item.font_size = size;
            }
        }
    }

    pub(crate) fn resize_from_bounds(&mut self, _from: Rect, _to: Rect) {}

    pub(crate) fn bounds(&self) -> Option<Rect> {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        let mut found = false;
        for item in &self.items {
            let Some(rect) = valid_rect(super::rect_from_tuple(item.rect)) else {
                continue;
            };
            found = true;
            min_x = min_x.min(rect.x);
            min_y = min_y.min(rect.y);
            max_x = max_x.max(rect.x + rect.width);
            max_y = max_y.max(rect.y + rect.height);
        }
        found.then_some(Rect {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(1.0),
            height: (max_y - min_y).max(1.0),
        })
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        for item in &mut self.items {
            item.rect.0 += dx;
            item.rect.1 += dy;
        }
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl TranslationAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    fn make_item(translated: &str) -> TranslationShapeItem {
        TranslationShapeItem {
            rect: (10.0, 10.0, 60.0, 30.0),
            original: "Hello".to_string(),
            translated: translated.to_string(),
            bg_color: (255, 0, 0, 255).into(),
            text_color: (0, 0, 0, 255).into(),
            font_size: 16.0,
            cover_mode: CoverMode::SolidColor,
        }
    }

    #[test]
    fn test_empty_translation_does_not_cover_image() {
        let mut pixmap = Pixmap::new(100, 60).expect("分配测试画布失败");
        pixmap.fill(Color::WHITE);
        TranslationAction::new(vec![make_item("")]).draw(&mut pixmap.as_mut());
        assert_eq!(pixmap.pixel(20, 20).map(|p| p.red()), Some(255));
    }

    #[test]
    fn test_identical_translation_does_not_cover_image() {
        let mut pixmap = Pixmap::new(100, 60).expect("分配测试画布失败");
        pixmap.fill(Color::WHITE);
        TranslationAction::new(vec![make_item("Hello")]).draw(&mut pixmap.as_mut());
        assert_eq!(pixmap.pixel(20, 20).map(|p| p.red()), Some(255));
    }
}
