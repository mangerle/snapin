//! 二级栏单个属性控件的分派与绘制。
//!
//! 本模块只读取工具栏状态，把控件按形状、描边、文字、遮挡与放大镜分组渲染；
//! 分组后每个函数只保留同类视觉规则，避免主循环演变成数百行 match。

use super::super::items::Tool;
use super::super::layout::{SubControl, SubDropdown};
use super::super::Toolbar;
use super::magnifier_controls::{render_magnifier_sub_control, MagnifierControlSpec};
use super::sub_bar_helpers::{
    is_badge_control, render_badge_control, render_caret, render_corner_radius_with_value,
    render_effect_strength_slider, render_fill_toggle, render_size_with_dot,
    render_sub_shape_ellipse, render_sub_shape_rect, render_toggle_glyph,
    spotlight_opacity_percent, SubBarGeometry,
};
use super::sub_bar_icons::{
    render_font_size_icon, render_opacity_icon, render_spotlight_outline_icon, SubBarIconSpec,
};
use super::widgets::{
    fill_button_highlight, fill_circle, render_arrow_head_preview, render_arrow_preview,
    render_divider, stroke_circle, stroke_sample, ArrowHeadPreviewSpec, ArrowPreviewSpec,
    CircleSpec, LineSampleSpec, PreviewSpan, StrokeCircleSpec,
};
use crate::geometry::Rect;
use snapin_annotation::SpotlightShape;
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{Color, PixmapMut};

/// 单个二级栏控件的渲染上下文。
pub(super) struct SubControlRenderer<'a, 'p> {
    pub(super) toolbar: &'a Toolbar,
    pub(super) pixmap: &'a mut PixmapMut<'p>,
    pub(super) rect: Rect,
    pub(super) center_y: f32,
    pub(super) scale: f32,
    pub(super) color: Color,
    pub(super) highlight: Color,
    pub(super) presets: [Color; 10],
    pub(super) selected_color_index: usize,
}

impl SubControlRenderer<'_, '_> {
    /// 提取当前控件共享的几何与高亮状态。
    pub(super) fn geometry(&self) -> SubBarGeometry {
        SubBarGeometry {
            rect: self.rect,
            center_y: self.center_y,
            scale: self.scale,
            highlight: self.highlight,
        }
    }

    /// 绘制一个控件；放大镜和序号使用专用布局，优先交给各自渲染器。
    pub(super) fn render(mut self, control: Option<SubControl>) {
        let Some(control) = control else {
            render_divider(
                self.pixmap,
                self.rect.x + self.rect.width / 2.0,
                self.center_y,
                self.scale,
            );
            return;
        };

        if render_magnifier_sub_control(
            self.pixmap,
            MagnifierControlSpec {
                toolbar: self.toolbar,
                control,
                geometry: self.geometry(),
            },
        ) || is_badge_control(control)
        {
            if is_badge_control(control) {
                let geometry = self.geometry();
                render_badge_control(self.toolbar, control, self.pixmap, geometry);
            }
            return;
        }

        self.render_grouped(control);
    }

    /// 按视觉领域把控件分派到对应的渲染组。
    fn render_grouped(&mut self, control: SubControl) {
        match control {
            SubControl::ShapeRect
            | SubControl::ShapeEllipse
            | SubControl::Size
            | SubControl::EffectStrength => self.render_shape(control),
            SubControl::LineStyle
            | SubControl::FillToggle
            | SubControl::CornerRadius
            | SubControl::ArrowStyle
            | SubControl::Color(_)
            | SubControl::FontSize
            | SubControl::StartArrow
            | SubControl::EndArrow => self.render_stroke(control),
            SubControl::Bold
            | SubControl::Italic
            | SubControl::TextStroke
            | SubControl::TextBackground
            | SubControl::MarkerShape
            | SubControl::MarkerBlendMode
            | SubControl::EraserMode
            | SubControl::ClearAnnotations
            | SubControl::LineMode
            | SubControl::ArrowEnabled
            | SubControl::MosaicBrush
            | SubControl::MosaicRectangle
            | SubControl::MosaicMode
            | SubControl::FontFamily
            | SubControl::TextBackgroundOpacity
            | SubControl::TextBackgroundPadding
            | SubControl::TextBackgroundCornerRadius => self.render_text_or_mode(control),
            SubControl::SpotlightShapeRect
            | SubControl::SpotlightShapeEllipse
            | SubControl::SpotlightOpacity
            | SubControl::SpotlightOutline => self.render_spotlight(control),
            SubControl::MagnifierShapeRect
            | SubControl::MagnifierShapeEllipse
            | SubControl::MagnifierScale
            | SubControl::MagnifierSmooth
            | SubControl::MagnifierShadow
            | SubControl::MagnifierConnector => {}
            SubControl::BadgeStyle | SubControl::BadgeSequence => {
                unreachable!("序号控件应由专用渲染器提前处理")
            }
        }
    }

    fn render_shape(&mut self, control: SubControl) {
        match control {
            SubControl::ShapeRect => self.render_shape_button(Tool::Rect, true),
            SubControl::ShapeEllipse => self.render_shape_button(Tool::Ellipse, false),
            SubControl::Size => {
                let geometry = self.geometry();
                render_size_with_dot(
                    self.pixmap,
                    geometry,
                    self.toolbar.sub_options.custom_stroke_width,
                    self.toolbar.focused_sub_control == Some(SubControl::Size),
                );
            }
            SubControl::EffectStrength => {
                let geometry = self.geometry();
                render_effect_strength_slider(
                    self.pixmap,
                    geometry,
                    self.toolbar.sub_options.effect_strength,
                    self.toolbar.focused_sub_control == Some(SubControl::EffectStrength),
                );
            }
            _ => unreachable!("形状组收到错误控件"),
        }
    }

    fn render_shape_button(&mut self, tool: Tool, rectangle: bool) {
        let active = self.toolbar.active_tool == Some(tool);
        if active {
            fill_button_highlight(self.pixmap, &self.rect, self.highlight, self.scale);
        }
        let geometry = self.geometry();
        if rectangle {
            render_sub_shape_rect(self.pixmap, geometry, active);
        } else {
            render_sub_shape_ellipse(self.pixmap, geometry, active);
        }
    }

    fn render_stroke(&mut self, control: SubControl) {
        match control {
            SubControl::LineStyle => self.render_line_style(),
            SubControl::FillToggle => {
                let geometry = self.geometry();
                render_fill_toggle(
                    self.pixmap,
                    geometry,
                    self.toolbar.sub_options.filled,
                    self.color,
                );
            }
            SubControl::CornerRadius => {
                let geometry = self.geometry();
                render_corner_radius_with_value(
                    self.pixmap,
                    geometry,
                    self.toolbar.sub_options.corner_radius,
                    self.toolbar.focused_sub_control == Some(SubControl::CornerRadius),
                );
            }
            SubControl::ArrowStyle => self.render_arrow_style(),
            SubControl::Color(index) => self.render_color(index),
            SubControl::FontSize => self.render_font_size(),
            SubControl::StartArrow => self.render_arrow_head(true),
            SubControl::EndArrow => self.render_arrow_head(false),
            _ => unreachable!("描边组收到错误控件"),
        }
    }

    fn render_line_style(&mut self) {
        let open = self.toolbar.open_dropdown == Some(SubDropdown::Line);
        self.highlight_if_open(open);
        stroke_sample(
            self.pixmap,
            LineSampleSpec {
                x0: self.rect.x + 8.0 * self.scale,
                x1: self.rect.x + self.rect.width - 14.0 * self.scale,
                y: self.center_y,
                width: 2.0 * self.scale,
                style: self.toolbar.sub_options.line_style,
                color: Color::from_rgba8(45, 48, 56, 255),
            },
        );
        self.render_caret_with_inset(8.0);
    }

    fn render_arrow_style(&mut self) {
        let open = self.toolbar.open_dropdown == Some(SubDropdown::Arrow);
        self.highlight_if_open(open);
        render_arrow_preview(
            self.pixmap,
            ArrowPreviewSpec {
                span: PreviewSpan {
                    x0: self.rect.x + 8.0 * self.scale,
                    x1: self.rect.x + self.rect.width - 14.0 * self.scale,
                    center_y: self.center_y,
                    scale: self.scale,
                    color: Color::from_rgba8(45, 48, 56, 255),
                },
                style: self.toolbar.sub_options.arrow_style,
            },
        );
        self.render_caret_with_inset(8.0);
    }

    fn render_arrow_head(&mut self, at_start: bool) {
        let style = if at_start {
            self.toolbar.sub_options.start_arrow_head
        } else {
            self.toolbar.sub_options.end_arrow_head
        };
        render_arrow_head_preview(
            self.pixmap,
            ArrowHeadPreviewSpec {
                span: PreviewSpan {
                    x0: self.rect.x + 10.0 * self.scale,
                    x1: self.rect.x + self.rect.width - 18.0 * self.scale,
                    center_y: self.center_y,
                    scale: self.scale,
                    color: Color::from_rgba8(45, 48, 56, 255),
                },
                style,
                at_start,
            },
        );
        self.render_caret();
    }

    fn render_color(&mut self, index: usize) {
        let Some(preset) = self.presets.get(index) else {
            return;
        };
        let cx = self.rect.x + self.rect.width / 2.0;
        let circle = CircleSpec {
            center_x: cx,
            center_y: self.center_y,
            radius: 8.0 * self.scale,
            color: *preset,
        };
        fill_circle(self.pixmap, circle);
        stroke_circle(
            self.pixmap,
            StrokeCircleSpec {
                circle: CircleSpec {
                    color: Color::from_rgba8(0, 0, 0, 32),
                    ..circle
                },
                width: self.scale,
            },
        );
        if self.selected_color_index == index {
            stroke_circle(
                self.pixmap,
                StrokeCircleSpec {
                    circle: CircleSpec {
                        radius: 10.5 * self.scale,
                        color: Color::from_rgba8(37, 99, 235, 255),
                        ..circle
                    },
                    width: 2.2 * self.scale,
                },
            );
        }
    }

    fn render_font_size(&mut self) {
        let open = self.toolbar.open_dropdown == Some(SubDropdown::FontSize);
        self.highlight_if_open(open);
        render_font_size_icon(self.pixmap, self.dropdown_icon_spec(open));
        self.render_caret();
    }

    fn render_spotlight(&mut self, control: SubControl) {
        match control {
            SubControl::SpotlightShapeRect => {
                let active = self.toolbar.sub_options.spotlight_shape == SpotlightShape::Rectangle;
                self.render_active_shape(active, true);
            }
            SubControl::SpotlightShapeEllipse => {
                let active = self.toolbar.sub_options.spotlight_shape == SpotlightShape::Ellipse;
                self.render_active_shape(active, false);
            }
            SubControl::SpotlightOpacity => {
                let focused =
                    self.toolbar.focused_sub_control == Some(SubControl::SpotlightOpacity);
                self.render_icon_value(
                    &format!(
                        "{}%",
                        spotlight_opacity_percent(self.toolbar.sub_options.spotlight_intensity)
                    ),
                    focused,
                    render_opacity_icon,
                );
            }
            SubControl::SpotlightOutline => {
                let active = self.toolbar.sub_options.spotlight_outline;
                self.highlight_if_active(active);
                render_spotlight_outline_icon(self.pixmap, self.icon_spec(active));
            }
            _ => unreachable!("聚光灯组收到错误控件"),
        }
    }

    fn render_active_shape(&mut self, active: bool, rectangle: bool) {
        if active {
            fill_button_highlight(self.pixmap, &self.rect, self.highlight, self.scale);
        }
        let geometry = self.geometry();
        if rectangle {
            render_sub_shape_rect(self.pixmap, geometry, active);
        } else {
            render_sub_shape_ellipse(self.pixmap, geometry, active);
        }
    }

    /// 绘制带当前数值的属性图标；数值可显示，图形负责表达属性语义。
    pub(super) fn render_icon_value(
        &mut self,
        value: &str,
        focused: bool,
        draw_icon: impl FnOnce(&mut PixmapMut, SubBarIconSpec),
    ) {
        self.highlight_if_active(focused);
        let spec = SubBarIconSpec {
            center_x: self.rect.x + 11.0 * self.scale,
            center_y: self.center_y,
            scale: self.scale,
            color: self.icon_color(focused),
        };
        draw_icon(self.pixmap, spec);
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_rgba(
                self.pixmap,
                value,
                TextDrawOptions::new(
                    self.rect.x + 21.0 * self.scale,
                    self.center_y - 6.0 * self.scale,
                    10.5 * self.scale,
                    icon_rgba(self.icon_color(focused)),
                ),
            );
        });
    }

    pub(super) fn icon_spec(&self, active: bool) -> SubBarIconSpec {
        SubBarIconSpec {
            center_x: self.rect.x + self.rect.width / 2.0,
            center_y: self.center_y,
            scale: self.scale,
            color: self.icon_color(active),
        }
    }

    pub(super) fn dropdown_icon_spec(&self, active: bool) -> SubBarIconSpec {
        SubBarIconSpec {
            center_x: self.rect.x + 13.0 * self.scale,
            center_y: self.center_y,
            scale: self.scale,
            color: self.icon_color(active),
        }
    }

    pub(super) fn icon_color(&self, active: bool) -> Color {
        if active {
            Color::from_rgba8(37, 99, 235, 255)
        } else {
            Color::from_rgba8(45, 48, 56, 255)
        }
    }

    pub(super) fn highlight_if_active(&mut self, active: bool) {
        if active {
            fill_button_highlight(self.pixmap, &self.rect, self.highlight, self.scale);
        }
    }

    pub(super) fn highlight_if_open(&mut self, open: bool) {
        self.highlight_if_active(open);
    }

    pub(super) fn toggle_glyph(&mut self, glyph: &'static str, active: bool) {
        let geometry = self.geometry();
        render_toggle_glyph(self.pixmap, geometry, glyph, active);
    }

    pub(super) fn render_caret(&mut self) {
        self.render_caret_with_inset(9.0);
    }

    fn render_caret_with_inset(&mut self, inset: f32) {
        render_caret(
            self.pixmap,
            self.rect.x + self.rect.width - inset * self.scale,
            self.center_y,
            self.scale,
        );
    }
}

fn icon_rgba(color: Color) -> (u8, u8, u8, u8) {
    (
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8,
        (color.alpha() * 255.0).round() as u8,
    )
}
