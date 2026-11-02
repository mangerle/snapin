//! 步骤数字序号气泡标号图元绘制实现。

use super::arrow::ArrowAction;
use super::rect::rounded_rect_path;
use super::{estimate_text_width, map_point, SizeParam};
use crate::geometry::Rect;
use crate::render::{
    with_thread_local_text_renderer, CenteredTextOptions, StyledTextDrawOptions, TextRenderer,
    TextStyleOptions,
};
use crate::style::{ArrowHeadStyle, ArrowStyle, BadgeSequence, BadgeStyle, ColorRgba, LineStyle};
use serde::{Deserialize, Serialize};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 步骤序号标号动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BadgeAction {
    pub center: (f32, f32),
    pub number: usize,
    pub color: ColorRgba,
    pub radius: f32,
    /// 底板样式：实心圆底或空心描边
    #[serde(default)]
    pub style: BadgeStyle,
    #[serde(default)]
    pub sequence: BadgeSequence,
    /// 随序号显示的注释文字。
    #[serde(default)]
    pub text: String,
    /// 注释文字字体族。
    #[serde(default)]
    pub font_family: Option<String>,
    /// 注释文字字号；为 0 时按序号半径自动推导。
    #[serde(default)]
    pub font_size: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    /// 注释文字颜色；为空时沿用序号颜色。
    #[serde(default)]
    pub text_color: Option<ColorRgba>,
    /// 注释文字描边宽度。
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
    #[serde(default = "default_corner_radius")]
    pub corner_radius: f32,
    /// 是否在注释文字下方绘制导向线。
    #[serde(default)]
    pub guide_line: bool,
    /// 序号与注释文字之间的箭头样式；为空时不绘制箭头。
    #[serde(default)]
    pub arrow_style: Option<ArrowStyle>,
}

fn default_stroke_color() -> ColorRgba {
    ColorRgba::opaque(255, 255, 255)
}

fn default_padding() -> f32 {
    4.0
}

fn default_corner_radius() -> f32 {
    4.0
}

impl Default for BadgeAction {
    fn default() -> Self {
        Self {
            center: (0.0, 0.0),
            number: 1,
            color: ColorRgba::opaque(239, 68, 68),
            radius: 12.0,
            style: BadgeStyle::default(),
            sequence: BadgeSequence::default(),
            text: String::new(),
            font_family: None,
            font_size: 0.0,
            bold: false,
            italic: false,
            text_color: None,
            stroke_width: 0.0,
            stroke_color: default_stroke_color(),
            background_color: None,
            background_opacity: 0,
            padding: default_padding(),
            corner_radius: default_corner_radius(),
            guide_line: false,
            arrow_style: None,
        }
    }
}

impl BadgeAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (cx, cy) = (self.center.0 + offset_x, self.center.1 + offset_y);
        let r = self.radius;

        if self.style == BadgeStyle::Outline {
            self.draw_outline(pixmap, cx, cy, r);
            self.draw_text_label(pixmap, cx, cy, r);
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
            self.draw_label(renderer, pixmap, cx, cy, r, (255, 255, 255, 255));
        });
        self.draw_text_label(pixmap, cx, cy, r);
    }

    /// 在序号右侧绘制可选注释文字、背景、导向线与箭头。
    fn draw_text_label(&self, pixmap: &mut PixmapMut, cx: f32, cy: f32, radius: f32) {
        let text = self.text.trim();
        if text.is_empty() {
            return;
        }
        let font_size = if self.font_size > 0.0 {
            self.font_size
        } else {
            (radius * 1.05).max(10.0)
        };
        let gap = (radius * 0.45).max(6.0);
        let text_x = cx + radius + gap;
        let text_y = cy - font_size * 0.62;
        let text_width = estimate_text_width(text, font_size).max(1.0);
        let text_height = font_size * 1.25;

        if self.guide_line {
            let mut builder = PathBuilder::new();
            builder.move_to(cx + radius, cy);
            builder.line_to(text_x - 2.0, cy);
            if let Some(path) = builder.finish() {
                let mut paint = Paint::default();
                paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, 190);
                pixmap.stroke_path(
                    &path,
                    &paint,
                    &Stroke {
                        width: 1.2,
                        ..Default::default()
                    },
                    Transform::identity(),
                    None,
                );
            }
        }

        if let Some(style) = self.arrow_style {
            ArrowAction {
                start: (text_x + text_width + 8.0, text_y + text_height / 2.0),
                end: (cx + radius + 2.0, cy),
                color: self.color,
                stroke_width: 2.0,
                style,
                line_style: LineStyle::Solid,
                start_head: ArrowHeadStyle::None,
                end_head: ArrowHeadStyle::Triangle,
            }
            .draw_offset(pixmap, 0.0, 0.0);
        }

        if let Some(background) = self.background_color {
            let width = text_width + self.padding * 2.0;
            let height = text_height + self.padding * 2.0;
            if let Some(path) = rounded_rect_path(
                text_x - self.padding,
                text_y - self.padding,
                width,
                height,
                self.corner_radius,
            ) {
                let mut paint = Paint::default();
                paint.set_color_rgba8(
                    background.0,
                    background.1,
                    background.2,
                    self.background_opacity,
                );
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        let rgba = self.text_color.unwrap_or(self.color).to_tuple();
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
                        text,
                        StyledTextDrawOptions {
                            x: text_x + dx * self.stroke_width,
                            y: text_y + dy * self.stroke_width,
                            font_size,
                            rgba: self.stroke_color.to_tuple(),
                            style,
                        },
                    );
                }
            }
            renderer.draw_text_styled_rgba(
                pixmap,
                text,
                StyledTextDrawOptions {
                    x: text_x,
                    y: text_y,
                    font_size,
                    rgba,
                    style,
                },
            );
        });
    }

    /// 按序号位数选择字号，并复用渲染层的通用几何居中能力。
    fn draw_label(
        &self,
        renderer: &mut TextRenderer,
        pixmap: &mut PixmapMut,
        cx: f32,
        cy: f32,
        radius: f32,
        rgba: (u8, u8, u8, u8),
    ) {
        let font_size = if self.number < 10 {
            radius * 1.35
        } else if self.number < 100 {
            radius * 1.05
        } else {
            radius * 0.8
        };
        renderer.draw_centered_bold_text(
            pixmap,
            &self.label_text(),
            CenteredTextOptions {
                center: (cx, cy),
                font_size,
                max_extent: radius * 4.0,
                rgba,
            },
        );
    }

    /// 返回按当前序号格式转换后的显示文本。
    fn label_text(&self) -> String {
        self.sequence.label(self.number)
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        match param {
            SizeParam::Stroke(width) => self.radius = (width * 3.0).clamp(10.0, 96.0),
            SizeParam::Font(size) => self.font_size = size,
        }
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        let r = self.radius.max(1.0);
        let mut bounds = Rect {
            x: self.center.0 - r,
            y: self.center.1 - r,
            width: r * 2.0,
            height: r * 2.0,
        };
        if !self.text.trim().is_empty() {
            let font_size = if self.font_size > 0.0 {
                self.font_size
            } else {
                (r * 1.05).max(10.0)
            };
            let text_x = self.center.0 + r + (r * 0.45).max(6.0);
            let text_width = estimate_text_width(self.text.trim(), font_size)
                + self.padding * 2.0
                + font_size * 2.5;
            let text_height = font_size * 1.25 + self.padding * 2.0;
            let right = (text_x + text_width).max(bounds.x + bounds.width);
            let top = (self.center.1 - text_height / 2.0).min(bounds.y);
            let bottom = (self.center.1 + text_height / 2.0).max(bounds.y + bounds.height);
            bounds.x = bounds.x.min(self.center.0 - r);
            bounds.y = top;
            bounds.width = right - bounds.x;
            bounds.height = bottom - bounds.y;
        }
        Some(bounds)
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.center.0 += dx;
        self.center.1 += dy;
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.center = map_point(from, to, self.center);
        let sx = if from.width.abs() > f32::EPSILON {
            to.width / from.width
        } else {
            1.0
        };
        let sy = if from.height.abs() > f32::EPSILON {
            to.height / from.height
        } else {
            1.0
        };
        self.radius = (self.radius * (sx + sy) * 0.5).clamp(6.0, 512.0);
        if self.font_size > 0.0 {
            self.font_size = (self.font_size * (sx + sy) * 0.5).clamp(8.0, 192.0);
        }
        self.padding = (self.padding * (sx + sy) * 0.5).clamp(0.0, 48.0);
        self.corner_radius = (self.corner_radius * (sx + sy) * 0.5).clamp(0.0, 48.0);
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
            self.draw_label(renderer, pixmap, cx, cy, r, self.color.to_tuple());
        });
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl BadgeAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
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
            color: (220, 38, 38, 255).into(),
            radius: 24.0,
            style,
            sequence: BadgeSequence::Number,
            ..BadgeAction::default()
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

    /// 序号注释必须把文字绘制在圆圈右侧，而不是只保留裸序号。
    #[test]
    fn test_badge_draws_text_label_outside_circle() {
        let mut pixmap = Pixmap::new(180, 80).expect("分配 Pixmap 成功");
        BadgeAction {
            center: (30.0, 40.0),
            radius: 20.0,
            number: 1,
            color: (220, 38, 38, 255).into(),
            style: BadgeStyle::Filled,
            sequence: BadgeSequence::Number,
            text: "注释文字".to_string(),
            font_size: 20.0,
            text_color: Some((20, 20, 20, 255).into()),
            guide_line: true,
            ..BadgeAction::default()
        }
        .draw(&mut pixmap.as_mut());

        let has_text_or_guide = (52..160)
            .any(|x| (20..60).any(|y| pixmap.pixel(x, y).is_some_and(|pixel| pixel.alpha() > 0)));
        assert!(has_text_or_guide, "序号右侧必须绘制注释文字或导向线");
    }

    /// 数字格式切换必须真正改变序号文本，而不是始终显示阿拉伯数字。
    #[test]
    fn test_badge_sequence_formats_label_text() {
        let mut badge = BadgeAction {
            number: 2,
            sequence: BadgeSequence::UppercaseLetter,
            ..BadgeAction::default()
        };
        assert_eq!(badge.label_text(), "B");
        badge.sequence = BadgeSequence::RomanUpper;
        assert_eq!(badge.label_text(), "II");
    }
}
