//! 序号气泡标签测量、导向线、箭头与文字绘制。

use super::{BadgeAction, BadgeCircle};
use crate::geometry::{Point, Rect};
use crate::render::{
    with_thread_local_text_renderer, CenteredTextOptions, StyledTextDrawOptions, TextRenderer,
    TextStyleOptions,
};
use crate::shapes::arrow::ArrowAction;
use crate::shapes::rect::{rounded_rect_path, RoundedRectSpec};
use crate::shapes::{estimate_text_width, fill_path_if_visible};
use crate::style::{ArrowHeadStyle, LineStyle};
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 序号注释文字的位置与尺寸。
struct BadgeLabelMetrics<'a> {
    text: &'a str,
    font_size: f32,
    text_x: f32,
    text_y: f32,
    text_width: f32,
    text_height: f32,
    style: TextStyleOptions<'a>,
}

/// 序号数字的一次性绘制请求。
pub(super) struct BadgeNumberDraw<'a, 'p> {
    pub(super) renderer: &'a mut TextRenderer,
    pub(super) pixmap: &'a mut PixmapMut<'p>,
    pub(super) circle: BadgeCircle,
    pub(super) rgba: (u8, u8, u8, u8),
}

/// 在序号右侧绘制可选注释文字、背景、导向线与箭头。
impl BadgeAction {
    pub(super) fn draw_text_label(&self, pixmap: &mut PixmapMut, circle: BadgeCircle) {
        let Some(metrics) = self.label_metrics(circle) else {
            return;
        };
        self.draw_label_guide(pixmap, circle, &metrics);
        self.draw_label_arrow(pixmap, circle, &metrics);
        self.draw_label_background(pixmap, &metrics);
        self.draw_label_text(pixmap, &metrics);
    }

    /// 计算注释文字的光学位置与包围尺寸。
    fn label_metrics(&self, circle: BadgeCircle) -> Option<BadgeLabelMetrics<'_>> {
        let text = self.text.trim();
        if text.is_empty() {
            return None;
        }
        let font_size = if self.font_size > 0.0 {
            self.font_size
        } else {
            (circle.radius * 1.05).max(10.0)
        };
        let text_x = circle.center_x + circle.radius + (circle.radius * 0.45).max(6.0);
        Some(BadgeLabelMetrics {
            text,
            font_size,
            text_x,
            text_y: circle.center_y - font_size * 0.62,
            text_width: estimate_text_width(text, font_size).max(1.0),
            text_height: font_size * 1.25,
            style: TextStyleOptions {
                bold: self.bold,
                italic: self.italic,
                font_family: self.font_family.as_deref(),
            },
        })
    }

    /// 绘制序号到注释文字之间的导向线。
    fn draw_label_guide(
        &self,
        pixmap: &mut PixmapMut,
        circle: BadgeCircle,
        metrics: &BadgeLabelMetrics<'_>,
    ) {
        if !self.guide_line {
            return;
        }
        let mut builder = PathBuilder::new();
        builder.move_to(circle.center_x + circle.radius, circle.center_y);
        builder.line_to(metrics.text_x - 2.0, circle.center_y);
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

    /// 绘制指向序号气泡的箭头。
    fn draw_label_arrow(
        &self,
        pixmap: &mut PixmapMut,
        circle: BadgeCircle,
        metrics: &BadgeLabelMetrics<'_>,
    ) {
        let Some(style) = self.arrow_style else {
            return;
        };
        ArrowAction {
            start: Point::new(
                metrics.text_x + metrics.text_width + 8.0,
                metrics.text_y + metrics.text_height / 2.0,
            ),
            end: Point::new(circle.center_x + circle.radius + 2.0, circle.center_y),
            color: self.color,
            stroke_width: 2.0,
            style,
            line_style: LineStyle::Solid,
            start_head: ArrowHeadStyle::None,
            end_head: ArrowHeadStyle::Triangle,
        }
        .draw_offset(pixmap, 0.0, 0.0);
    }

    /// 绘制注释文字的圆角背景。
    fn draw_label_background(&self, pixmap: &mut PixmapMut, metrics: &BadgeLabelMetrics<'_>) {
        let Some(background) = self.background_color else {
            return;
        };
        let width = metrics.text_width + self.padding * 2.0;
        let height = metrics.text_height + self.padding * 2.0;
        let Some(path) = rounded_rect_path(RoundedRectSpec {
            rect: Rect::new(
                metrics.text_x - self.padding,
                metrics.text_y - self.padding,
                width,
                height,
            ),
            radius: self.corner_radius,
        }) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            background.0,
            background.1,
            background.2,
            self.background_opacity,
        );
        fill_path_if_visible(
            pixmap,
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// 绘制注释文字及其可选描边。
    fn draw_label_text(&self, pixmap: &mut PixmapMut, metrics: &BadgeLabelMetrics<'_>) {
        let rgba = self.text_color.unwrap_or(self.color).to_tuple();
        with_thread_local_text_renderer(|renderer| {
            if self.stroke_width > 0.0 {
                self.draw_text_halo(renderer, pixmap, metrics);
            }
            renderer.draw_text_styled_rgba(
                pixmap,
                metrics.text,
                StyledTextDrawOptions {
                    x: metrics.text_x,
                    y: metrics.text_y,
                    font_size: metrics.font_size,
                    rgba,
                    style: metrics.style,
                },
            );
        });
    }

    /// 绘制八方向描边晕影。
    fn draw_text_halo(
        &self,
        renderer: &mut TextRenderer,
        pixmap: &mut PixmapMut,
        metrics: &BadgeLabelMetrics<'_>,
    ) {
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
                metrics.text,
                StyledTextDrawOptions {
                    x: metrics.text_x + dx * self.stroke_width,
                    y: metrics.text_y + dy * self.stroke_width,
                    font_size: metrics.font_size,
                    rgba: self.stroke_color.to_tuple(),
                    style: metrics.style,
                },
            );
        }
    }

    /// 按序号位数选择字号，并复用渲染层的通用几何居中能力。
    pub(super) fn draw_label(&self, request: BadgeNumberDraw<'_, '_>) {
        let BadgeNumberDraw {
            renderer,
            pixmap,
            circle,
            rgba,
        } = request;
        let radius = circle.radius;
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
                center: (circle.center_x, circle.center_y),
                font_size,
                max_extent: radius * 4.0,
                rgba,
            },
        );
    }

    /// 返回按当前序号格式转换后的显示文本。
    pub(super) fn label_text(&self) -> String {
        self.sequence.label(self.number)
    }
}
