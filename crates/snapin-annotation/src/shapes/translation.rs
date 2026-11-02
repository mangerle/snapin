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
        self.draw_backgrounds(pixmap, offset_x, offset_y);
        self.draw_translations(pixmap, offset_x, offset_y);
    }

    /// 批量绘制译文背景擦除层。
    fn draw_backgrounds(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        for item in &self.items {
            if !has_effective_translation(item) {
                continue;
            }
            let rx = item.rect.0 + offset_x;
            let ry = item.rect.1 + offset_y;
            let rw = item.rect.2;
            let rh = item.rect.3;
            if rw <= 1.0 || rh <= 1.0 {
                continue;
            }
            let alpha = match item.cover_mode {
                CoverMode::FrostedGlass => 180,
                CoverMode::SolidColor => 255,
            };
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba8(
                item.bg_color.0,
                item.bg_color.1,
                item.bg_color.2,
                alpha,
            ));
            paint.anti_alias = true;
            if let Some(path) = rounded_rect_path(rx, ry, rw, rh) {
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    /// 批量绘制有效译文文本。
    fn draw_translations(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        use crate::render::TextBoxOptions;
        with_thread_local_text_renderer(|renderer| {
            for item in &self.items {
                if !has_effective_translation(item) {
                    continue;
                }
                let rx = item.rect.0 + offset_x + 2.0;
                let ry = item.rect.1 + offset_y + 2.0;
                renderer.draw_text_box_rgba(
                    pixmap,
                    &item.translated,
                    TextBoxOptions {
                        rect: (
                            rx,
                            ry,
                            (item.rect.2 - 4.0).max(10.0),
                            (item.rect.3 - 4.0).max(10.0),
                        ),
                        font_size: item.font_size.max(9.0),
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

/// 判断译文是否需要覆盖原文并重新排版。
fn has_effective_translation(item: &TranslationShapeItem) -> bool {
    let translated = item.translated.trim();
    !translated.is_empty() && translated != item.original.trim()
}

/// 生成译文背景使用的平滑微圆角矩形路径。
fn rounded_rect_path(x: f32, y: f32, width: f32, height: f32) -> Option<tiny_skia::Path> {
    let radius = 3.0_f32.min(width / 2.0).min(height / 2.0);
    let mut builder = PathBuilder::new();
    builder.move_to(x + radius, y);
    builder.line_to(x + width - radius, y);
    builder.quad_to(x + width, y, x + width, y + radius);
    builder.line_to(x + width, y + height - radius);
    builder.quad_to(x + width, y + height, x + width - radius, y + height);
    builder.line_to(x + radius, y + height);
    builder.quad_to(x, y + height, x, y + height - radius);
    builder.line_to(x, y + radius);
    builder.quad_to(x, y, x + radius, y);
    builder.close();
    builder.finish()
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
