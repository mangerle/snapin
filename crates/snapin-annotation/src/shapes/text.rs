//! 文本标注图元绘制实现。

use super::rect::rounded_rect_path;
use super::{estimate_text_width, map_point, SizeParam};
use crate::geometry::Rect;
use crate::render::{with_thread_local_text_renderer, StyledTextDrawOptions, TextStyleOptions};
use crate::style::ColorRgba;
use serde::{Deserialize, Serialize};
use tiny_skia::{Color, FillRule, Paint, PixmapMut, Transform};

/// 文本标注图元动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextAction {
    pub pos: (f32, f32),
    pub text: String,
    pub color: ColorRgba,
    pub font_size: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub stroke_width: f32,
    #[serde(default = "default_stroke_color")]
    pub stroke_color: ColorRgba,
    #[serde(default)]
    pub background_color: Option<ColorRgba>,
    #[serde(default)]
    pub background_opacity: u8,
    #[serde(default = "default_padding")]
    pub padding: f32,
    #[serde(default = "default_corner")]
    pub corner_radius: f32,
}

fn default_stroke_color() -> ColorRgba {
    ColorRgba::opaque(255, 255, 255)
}

fn default_padding() -> f32 {
    4.0
}

fn default_corner() -> f32 {
    4.0
}

impl TextAction {
    /// 使用默认文字样式构造基础文本。
    pub fn plain(pos: (f32, f32), text: String, color: ColorRgba, font_size: f32) -> Self {
        Self {
            pos,
            text,
            color,
            font_size,
            bold: false,
            italic: false,
            font_family: None,
            stroke_width: 0.0,
            stroke_color: default_stroke_color(),
            background_color: None,
            background_opacity: 0,
            padding: default_padding(),
            corner_radius: default_corner(),
        }
    }

    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        if self.text.is_empty() {
            return;
        }

        let (x, y) = (self.pos.0 + offset_x, self.pos.1 + offset_y);
        let font_size = self.font_size.max(8.0);
        if let Some(background) = self.background_color {
            let width = estimate_text_width(&self.text, font_size) + self.padding * 2.0;
            let height = font_size * 1.25 + self.padding * 2.0;
            let rect_x = x - self.padding;
            let rect_y = y - self.padding;
            if let Some(path) = rounded_rect_path(rect_x, rect_y, width, height, self.corner_radius)
            {
                let mut paint = Paint::default();
                paint.set_color(Color::from_rgba8(
                    background.0,
                    background.1,
                    background.2,
                    self.background_opacity,
                ));
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        with_thread_local_text_renderer(|renderer| {
            let style = TextStyleOptions {
                bold: self.bold,
                italic: self.italic,
                font_family: self.font_family.as_deref(),
            };
            if self.stroke_width > 0.0 {
                for (dx, dy) in [
                    (-1.0, -1.0),
                    (0.0, -1.0),
                    (1.0, -1.0),
                    (-1.0, 0.0),
                    (1.0, 0.0),
                    (-1.0, 1.0),
                    (0.0, 1.0),
                    (1.0, 1.0),
                ] {
                    renderer.draw_text_styled_rgba(
                        pixmap,
                        &self.text,
                        StyledTextDrawOptions {
                            x: x + dx * self.stroke_width,
                            y: y + dy * self.stroke_width,
                            font_size,
                            rgba: self.stroke_color.to_tuple(),
                            style,
                        },
                    );
                }
            }
            renderer.draw_text_styled_rgba(
                pixmap,
                &self.text,
                StyledTextDrawOptions {
                    x,
                    y,
                    font_size,
                    rgba: self.color.to_tuple(),
                    style,
                },
            );
        });
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Font(size) = param {
            self.font_size = size;
        }
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        if self.text.is_empty() {
            return None;
        }
        let width = estimate_text_width(&self.text, self.font_size).max(1.0);
        let width = width + self.padding * 2.0;
        let height = (self.font_size * 1.25 + self.padding * 2.0).max(1.0);
        Some(Rect {
            x: self.pos.0 - self.padding,
            y: self.pos.1 - self.padding,
            width,
            height,
        })
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.pos.0 += dx;
        self.pos.1 += dy;
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.pos = map_point(from, to, self.pos);
        let sy = if from.height.abs() > f32::EPSILON {
            to.height / from.height
        } else {
            1.0
        };
        self.font_size = (self.font_size * sy).clamp(8.0, 240.0);
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl TextAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    /// 文字背景必须使用圆角路径，左上角应保持透明而边缘中部应着色。
    #[test]
    fn test_text_background_uses_rounded_corners() {
        let mut pixmap = Pixmap::new(96, 64).expect("分配 Pixmap 成功");
        let mut action = TextAction::plain(
            (24.0, 20.0),
            "A".to_string(),
            (20, 20, 20, 255).into(),
            24.0,
        );
        action.background_color = Some((240, 240, 240, 255).into());
        action.background_opacity = 255;
        action.padding = 8.0;
        action.corner_radius = 12.0;
        action.draw(&mut pixmap.as_mut());

        let alpha_at = |x: u32, y: u32| pixmap.pixel(x, y).map(|pixel| pixel.alpha()).unwrap_or(0);
        assert_eq!(alpha_at(16, 12), 0, "背景圆角外侧应保持透明");
        assert!(alpha_at(16, 20) > 0, "背景左侧边缘应着色");
    }
}
