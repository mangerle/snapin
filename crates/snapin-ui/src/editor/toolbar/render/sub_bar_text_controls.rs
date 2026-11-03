//! 二级栏文字样式与模式下拉渲染。

use super::super::items::LineMode;
use super::super::layout::{SubControl, SubDropdown};
use super::sub_bar_controls::SubControlRenderer;
use super::sub_bar_helpers::render_corner_radius_with_value;
use super::sub_bar_icons::{
    render_arrow_toggle_icon, render_clear_annotations_icon, render_eraser_mode_icon,
    render_font_family_icon, render_line_mode_icon, render_marker_blend_icon,
    render_marker_shape_icon, render_mosaic_mode_icon, render_mosaic_shape_icon,
    render_opacity_icon, render_padding_icon, render_text_background_icon, render_text_stroke_icon,
};
use super::widgets::fill_button_highlight;
use tiny_skia::Color;

impl SubControlRenderer<'_, '_> {
    pub(super) fn render_text_or_mode(&mut self, control: SubControl) {
        match control {
            SubControl::Bold
            | SubControl::Italic
            | SubControl::TextStroke
            | SubControl::TextBackground => self.render_text_style(control),
            SubControl::MarkerShape => {
                let open = self.toolbar.open_dropdown == Some(SubDropdown::MarkerShape);
                self.highlight_if_open(open);
                render_marker_shape_icon(
                    self.pixmap,
                    self.dropdown_icon_spec(open),
                    self.toolbar.sub_options.marker_shape,
                );
                self.render_caret();
            }
            SubControl::MarkerBlendMode => {
                let open = self.toolbar.open_dropdown == Some(SubDropdown::MarkerBlendMode);
                self.highlight_if_open(open);
                render_marker_blend_icon(
                    self.pixmap,
                    self.dropdown_icon_spec(open),
                    self.toolbar.sub_options.marker_blend_mode,
                );
                self.render_caret();
            }
            SubControl::EraserBrush => {
                let active = !self.toolbar.sub_options.eraser_rectangle_mode;
                self.highlight_if_active(active);
                render_eraser_mode_icon(self.pixmap, self.icon_spec(active), false);
            }
            SubControl::EraserRectangle => {
                let active = self.toolbar.sub_options.eraser_rectangle_mode;
                self.highlight_if_active(active);
                render_eraser_mode_icon(self.pixmap, self.icon_spec(active), true);
            }
            SubControl::ClearAnnotations => {
                if self.toolbar.hovered_sub_control == Some(SubControl::ClearAnnotations) {
                    fill_button_highlight(
                        self.pixmap,
                        &self.rect,
                        Color::from_rgba8(239, 68, 68, 42),
                        self.scale,
                    );
                }
                render_clear_annotations_icon(self.pixmap, self.icon_spec(false));
            }
            SubControl::LineMode => {
                let polyline = self.toolbar.sub_options.line_mode == LineMode::Polyline;
                self.highlight_if_active(polyline);
                render_line_mode_icon(
                    self.pixmap,
                    self.icon_spec(polyline),
                    self.toolbar.sub_options.line_mode,
                );
            }
            SubControl::ArrowEnabled => {
                let active = self.toolbar.sub_options.line_arrow_enabled;
                self.highlight_if_active(active);
                render_arrow_toggle_icon(self.pixmap, self.icon_spec(active), active);
            }
            SubControl::MosaicBrush => {
                let active = self.toolbar.sub_options.mosaic_brush_mode;
                self.highlight_if_active(active);
                render_mosaic_shape_icon(self.pixmap, self.icon_spec(active), true);
            }
            SubControl::MosaicRectangle => {
                let active = !self.toolbar.sub_options.mosaic_brush_mode;
                self.highlight_if_active(active);
                render_mosaic_shape_icon(self.pixmap, self.icon_spec(active), false);
            }
            SubControl::MosaicMode => {
                let open = self.toolbar.open_dropdown == Some(SubDropdown::MosaicMode);
                self.highlight_if_open(open);
                render_mosaic_mode_icon(
                    self.pixmap,
                    self.dropdown_icon_spec(open),
                    self.toolbar.sub_options.mosaic_mode,
                );
                self.render_caret();
            }
            SubControl::FontFamily
            | SubControl::TextBackgroundOpacity
            | SubControl::TextBackgroundPadding
            | SubControl::TextBackgroundCornerRadius => self.render_text_background(control),
            _ => unreachable!("文本或模式组收到错误控件"),
        }
    }

    /// 文本粗体、斜体、描边与背景开关。
    pub(super) fn render_text_style(&mut self, control: SubControl) {
        match control {
            SubControl::Bold => self.toggle_glyph("B", self.toolbar.sub_options.text_bold),
            SubControl::Italic => self.toggle_glyph("I", self.toolbar.sub_options.text_italic),
            SubControl::TextStroke => {
                let active = self.toolbar.sub_options.text_stroke_width > 0.0;
                self.highlight_if_active(active);
                render_text_stroke_icon(self.pixmap, self.icon_spec(active));
            }
            SubControl::TextBackground => {
                let active = self.toolbar.sub_options.text_background_enabled;
                self.highlight_if_active(active);
                render_text_background_icon(self.pixmap, self.icon_spec(active));
            }
            _ => unreachable!("文本样式组收到错误控件"),
        }
    }

    /// 字体族与文字背景数值控件。
    pub(super) fn render_text_background(&mut self, control: SubControl) {
        match control {
            SubControl::FontFamily => {
                let open = self.toolbar.open_dropdown == Some(SubDropdown::FontFamily);
                self.highlight_if_open(open);
                render_font_family_icon(self.pixmap, self.dropdown_icon_spec(open));
                self.render_caret();
            }
            SubControl::TextBackgroundOpacity => {
                let focused =
                    self.toolbar.focused_sub_control == Some(SubControl::TextBackgroundOpacity);
                self.render_icon_value(
                    &format!("{}%", self.toolbar.sub_options.text_background_opacity),
                    focused,
                    render_opacity_icon,
                );
            }
            SubControl::TextBackgroundPadding => {
                let focused =
                    self.toolbar.focused_sub_control == Some(SubControl::TextBackgroundPadding);
                self.render_icon_value(
                    &format!("{:.0}", self.toolbar.sub_options.text_background_padding),
                    focused,
                    render_padding_icon,
                );
            }
            SubControl::TextBackgroundCornerRadius => {
                let geometry = self.geometry();
                render_corner_radius_with_value(
                    self.pixmap,
                    geometry,
                    self.toolbar.sub_options.text_background_corner_radius,
                    self.toolbar.focused_sub_control
                        == Some(SubControl::TextBackgroundCornerRadius),
                );
            }
            _ => unreachable!("文字背景组收到错误控件"),
        }
    }
}
