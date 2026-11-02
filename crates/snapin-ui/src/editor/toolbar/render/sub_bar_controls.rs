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
    render_effect_strength_slider, render_fill_toggle, render_numeric, render_size_with_dot,
    render_sub_shape_ellipse, render_sub_shape_rect, render_toggle_text, spotlight_opacity_percent,
};
use super::widgets::{
    fill_button_highlight, fill_circle, render_arrow_head_preview, render_arrow_preview,
    render_divider, stroke_circle, stroke_sample, ArrowHeadPreviewSpec,
};
use crate::geometry::Rect;
use snapin_annotation::SpotlightShape;
use snapin_render::with_thread_local_text_renderer;
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
    /// 绘制一个控件；放大镜和序号使用专用布局，优先交给各自渲染器。
    pub(super) fn render(mut self, control: Option<SubControl>) {
        let Some(control) = control else {
            render_divider(
                self.pixmap,
                self.rect.x + 8.0 * self.scale,
                self.rect.y + 8.0 * self.scale,
                self.scale,
            );
            return;
        };

        if render_magnifier_sub_control(
            self.pixmap,
            MagnifierControlSpec {
                toolbar: self.toolbar,
                control,
                rect: &self.rect,
                center_y: self.center_y,
                scale: self.scale,
                highlight: self.highlight,
            },
        ) || is_badge_control(control)
        {
            if is_badge_control(control) {
                render_badge_control(
                    self.toolbar,
                    control,
                    self.pixmap,
                    &self.rect,
                    self.center_y,
                    self.scale,
                    self.highlight,
                );
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
            SubControl::Size => render_size_with_dot(
                self.pixmap,
                &self.rect,
                self.center_y,
                self.scale,
                self.toolbar.sub_options.custom_stroke_width,
                self.toolbar.focused_sub_control == Some(SubControl::Size),
            ),
            SubControl::EffectStrength => render_effect_strength_slider(
                self.pixmap,
                &self.rect,
                self.center_y,
                self.scale,
                self.toolbar.sub_options.effect_strength,
                self.toolbar.focused_sub_control == Some(SubControl::EffectStrength),
            ),
            _ => unreachable!("形状组收到错误控件"),
        }
    }

    fn render_shape_button(&mut self, tool: Tool, rectangle: bool) {
        let active = self.toolbar.active_tool == Some(tool);
        if active {
            fill_button_highlight(self.pixmap, &self.rect, self.highlight, self.scale);
        }
        let x = self.rect.x + 12.0 * self.scale;
        if rectangle {
            render_sub_shape_rect(self.pixmap, x, self.center_y, self.scale, active);
        } else {
            render_sub_shape_ellipse(self.pixmap, x, self.center_y, self.scale, active);
        }
    }

    fn render_stroke(&mut self, control: SubControl) {
        match control {
            SubControl::LineStyle => self.render_line_style(),
            SubControl::FillToggle => render_fill_toggle(
                self.pixmap,
                self.rect.x + 12.0 * self.scale,
                self.center_y,
                self.scale,
                self.toolbar.sub_options.filled,
                self.color,
            ),
            SubControl::CornerRadius => render_corner_radius_with_value(
                self.pixmap,
                &self.rect,
                self.center_y,
                self.scale,
                self.toolbar.sub_options.corner_radius,
                self.toolbar.focused_sub_control == Some(SubControl::CornerRadius),
            ),
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
            self.rect.x + 8.0 * self.scale,
            self.rect.x + self.rect.width - 14.0 * self.scale,
            self.center_y,
            2.0 * self.scale,
            self.toolbar.sub_options.line_style,
            Color::from_rgba8(45, 48, 56, 255),
        );
        self.render_caret_with_inset(8.0);
    }

    fn render_arrow_style(&mut self) {
        let open = self.toolbar.open_dropdown == Some(SubDropdown::Arrow);
        self.highlight_if_open(open);
        render_arrow_preview(
            self.pixmap,
            self.rect.x + 8.0 * self.scale,
            self.rect.x + self.rect.width - 14.0 * self.scale,
            self.center_y,
            self.scale,
            self.toolbar.sub_options.arrow_style,
            Color::from_rgba8(45, 48, 56, 255),
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
                x0: self.rect.x + 10.0 * self.scale,
                x1: self.rect.x + self.rect.width - 18.0 * self.scale,
                cy: self.center_y,
                scale: self.scale,
                style,
                color: Color::from_rgba8(45, 48, 56, 255),
                at_start,
            },
        );
        self.render_caret();
    }

    fn render_color(&mut self, index: usize) {
        let Some(preset) = self.presets.get(index) else {
            return;
        };
        let cx = self.rect.x + 8.0 * self.scale;
        fill_circle(self.pixmap, cx, self.center_y, 8.0 * self.scale, *preset);
        stroke_circle(
            self.pixmap,
            cx,
            self.center_y,
            8.0 * self.scale,
            Color::from_rgba8(0, 0, 0, 32),
            self.scale,
        );
        if self.selected_color_index == index {
            stroke_circle(
                self.pixmap,
                cx,
                self.center_y,
                10.5 * self.scale,
                Color::from_rgba8(37, 99, 235, 255),
                2.2 * self.scale,
            );
        }
    }

    fn render_font_size(&mut self) {
        let open = self.toolbar.open_dropdown == Some(SubDropdown::FontSize);
        self.highlight_if_open(open);
        let label = format!("A {}", self.toolbar.sub_options.font_size.round() as u32);
        self.render_text(&label, 12.0);
        self.render_caret();
    }

    fn render_text_or_mode(&mut self, control: SubControl) {
        match control {
            SubControl::Bold
            | SubControl::Italic
            | SubControl::TextStroke
            | SubControl::TextBackground => self.render_text_style(control),
            SubControl::MarkerShape => self.render_dropdown_label(
                SubDropdown::MarkerShape,
                snapin_foundation::i18n::tr(self.toolbar.sub_options.marker_shape.label()),
            ),
            SubControl::MarkerBlendMode => self.render_dropdown_label(
                SubDropdown::MarkerBlendMode,
                snapin_foundation::i18n::tr(self.toolbar.sub_options.marker_blend_mode.label()),
            ),
            SubControl::EraserMode => self.toggle_text(
                if self.toolbar.sub_options.eraser_rectangle_mode {
                    "框选"
                } else {
                    "画笔"
                },
                true,
            ),
            SubControl::ClearAnnotations => self.toggle_text("清空", false),
            SubControl::LineMode => {
                self.toggle_text(self.toolbar.sub_options.line_mode.label(), true)
            }
            SubControl::ArrowEnabled => {
                self.toggle_text("箭头", self.toolbar.sub_options.line_arrow_enabled)
            }
            SubControl::MosaicBrush => {
                self.toggle_text("画笔", self.toolbar.sub_options.mosaic_brush_mode)
            }
            SubControl::MosaicRectangle => {
                self.toggle_text("矩形", !self.toolbar.sub_options.mosaic_brush_mode)
            }
            SubControl::MosaicMode => self.render_dropdown_label(
                SubDropdown::MosaicMode,
                snapin_foundation::i18n::tr(self.toolbar.sub_options.mosaic_mode.label()),
            ),
            SubControl::FontFamily
            | SubControl::TextBackgroundOpacity
            | SubControl::TextBackgroundPadding
            | SubControl::TextBackgroundCornerRadius => self.render_text_background(control),
            _ => unreachable!("文本或模式组收到错误控件"),
        }
    }

    /// 文本粗体、斜体、描边与背景开关。
    fn render_text_style(&mut self, control: SubControl) {
        match control {
            SubControl::Bold => self.toggle_text("B", self.toolbar.sub_options.text_bold),
            SubControl::Italic => self.toggle_text("I", self.toolbar.sub_options.text_italic),
            SubControl::TextStroke => {
                self.toggle_text("描边", self.toolbar.sub_options.text_stroke_width > 0.0)
            }
            SubControl::TextBackground => {
                self.toggle_text("背景", self.toolbar.sub_options.text_background_enabled)
            }
            _ => unreachable!("文本样式组收到错误控件"),
        }
    }

    /// 字体族与文字背景数值控件。
    fn render_text_background(&mut self, control: SubControl) {
        match control {
            SubControl::FontFamily => {
                let family = self
                    .toolbar
                    .sub_options
                    .font_family
                    .map(snapin_foundation::i18n::tr)
                    .unwrap_or_else(|| snapin_foundation::i18n::tr("系统默认"));
                self.render_dropdown_label_with_size(SubDropdown::FontFamily, family, 11.0);
            }
            SubControl::TextBackgroundOpacity => self.render_numeric_enabled(&format!(
                "{}%",
                self.toolbar.sub_options.text_background_opacity
            )),
            SubControl::TextBackgroundPadding => self.render_numeric_enabled(&format!(
                "{} {:.0}",
                snapin_foundation::i18n::tr("边距"),
                self.toolbar.sub_options.text_background_padding
            )),
            SubControl::TextBackgroundCornerRadius => self.render_numeric_enabled(&format!(
                "{} {:.0}",
                snapin_foundation::i18n::tr("圆角"),
                self.toolbar.sub_options.text_background_corner_radius
            )),
            _ => unreachable!("文字背景组收到错误控件"),
        }
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
            SubControl::SpotlightOpacity => render_numeric(
                self.pixmap,
                &self.rect,
                self.center_y,
                self.scale,
                &format!(
                    "{}%",
                    spotlight_opacity_percent(self.toolbar.sub_options.spotlight_intensity)
                ),
                true,
            ),
            SubControl::SpotlightOutline => {
                self.toggle_text("描边", self.toolbar.sub_options.spotlight_outline)
            }
            _ => unreachable!("聚光灯组收到错误控件"),
        }
    }

    fn render_active_shape(&mut self, active: bool, rectangle: bool) {
        if active {
            fill_button_highlight(self.pixmap, &self.rect, self.highlight, self.scale);
        }
        let x = self.rect.x + 12.0 * self.scale;
        if rectangle {
            render_sub_shape_rect(self.pixmap, x, self.center_y, self.scale, active);
        } else {
            render_sub_shape_ellipse(self.pixmap, x, self.center_y, self.scale, active);
        }
    }

    fn render_numeric_enabled(&mut self, text: &str) {
        render_numeric(
            self.pixmap,
            &self.rect,
            self.center_y,
            self.scale,
            text,
            self.toolbar.sub_options.text_background_enabled,
        );
    }

    fn render_dropdown_label(&mut self, dropdown: SubDropdown, label: &str) {
        self.render_dropdown_label_with_size(dropdown, label, 12.0);
    }

    fn render_dropdown_label_with_size(
        &mut self,
        dropdown: SubDropdown,
        label: &str,
        font_size: f32,
    ) {
        let open = self.toolbar.open_dropdown == Some(dropdown);
        self.highlight_if_open(open);
        self.render_text(label, font_size);
        self.render_caret();
    }

    fn render_text(&mut self, text: &str, font_size: f32) {
        let x = self.rect.x + 7.0 * self.scale;
        let y = self.rect.y + 5.0 * self.scale;
        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_rgba(
                self.pixmap,
                text,
                x,
                y,
                font_size * self.scale,
                (45, 48, 56, 255),
            );
        });
    }

    fn highlight_if_open(&mut self, open: bool) {
        if open {
            fill_button_highlight(self.pixmap, &self.rect, self.highlight, self.scale);
        }
    }

    fn toggle_text(&mut self, text: &'static str, active: bool) {
        render_toggle_text(self.pixmap, &self.rect, self.scale, text, active);
    }

    fn render_caret(&mut self) {
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
