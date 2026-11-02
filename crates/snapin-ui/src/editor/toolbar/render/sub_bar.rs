//! 二级属性栏与属性控件绘制。

use super::super::items::{closest_preset_color_index, preset_colors};
use super::super::layout::{SubControl, SubDropdown};
use super::super::Toolbar;
use super::dropdown::render_dropdown;
use super::magnifier_controls::render_magnifier_sub_control;
use super::sub_bar_helpers::{
    is_badge_control, render_badge_control, render_caret, render_corner_radius_with_value,
    render_effect_strength_slider, render_fill_toggle, render_numeric, render_size_with_dot,
    render_sub_shape_ellipse, render_sub_shape_rect, render_toggle_text, spotlight_opacity_percent,
};
use super::widgets::{
    fill_button_highlight, fill_circle, render_arrow_head_preview, render_arrow_preview,
    render_divider, stroke_circle, stroke_sample, ArrowHeadPreviewSpec,
};
use crate::editor::toolbar::items::Tool;
use crate::geometry::Rect;
use snapin_core::annotation::SpotlightShape;
use snapin_core::render::with_thread_local_text_renderer;
use tiny_skia::{Color, PixmapMut};

/// 绘制二级属性栏内容（形状、数值尺寸、线型、颜色与工具专属控件，自适应 HiDPI 缩放）
pub(super) fn render_sub_options(toolbar: &Toolbar, pixmap: &mut PixmapMut, rect: &Rect) {
    let Some(layout) = toolbar.sub_layout() else {
        return;
    };
    let s = toolbar.ui_scale;
    let center_y = rect.y + rect.height / 2.0;
    let color = toolbar.sub_options.color;
    let highlight = Color::from_rgba8(37, 99, 235, 45);
    let presets = preset_colors();
    let selected_color_index = closest_preset_color_index(color);

    for entry in &layout.entries {
        if let Some(control) = entry.control {
            if render_magnifier_sub_control(
                toolbar,
                control,
                pixmap,
                &entry.rect,
                center_y,
                s,
                highlight,
            ) {
                continue;
            }
            if is_badge_control(control) {
                render_badge_control(
                    toolbar,
                    control,
                    pixmap,
                    &entry.rect,
                    center_y,
                    s,
                    highlight,
                );
                continue;
            }
        }
        match entry.control {
            None => render_divider(pixmap, entry.rect.x + 8.0 * s, rect.y + 8.0 * s, s),
            Some(SubControl::ShapeRect) => {
                let active = toolbar.active_tool == Some(Tool::Rect);
                if active {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_sub_shape_rect(pixmap, entry.rect.x + 12.0 * s, center_y, s, active);
            }
            Some(SubControl::ShapeEllipse) => {
                let active = toolbar.active_tool == Some(Tool::Ellipse);
                if active {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_sub_shape_ellipse(pixmap, entry.rect.x + 12.0 * s, center_y, s, active);
            }
            Some(SubControl::Size) => {
                render_size_with_dot(
                    pixmap,
                    &entry.rect,
                    center_y,
                    s,
                    toolbar.sub_options.custom_stroke_width,
                    toolbar.focused_sub_control == Some(SubControl::Size),
                );
            }
            Some(SubControl::EffectStrength) => render_effect_strength_slider(
                pixmap,
                &entry.rect,
                center_y,
                s,
                toolbar.sub_options.effect_strength,
                toolbar.focused_sub_control == Some(SubControl::EffectStrength),
            ),
            Some(SubControl::LineStyle) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::Line);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                stroke_sample(
                    pixmap,
                    entry.rect.x + 8.0 * s,
                    entry.rect.x + entry.rect.width - 14.0 * s,
                    center_y,
                    2.0 * s,
                    toolbar.sub_options.line_style,
                    Color::from_rgba8(45, 48, 56, 255),
                );
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 8.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::FillToggle) => {
                render_fill_toggle(
                    pixmap,
                    entry.rect.x + 12.0 * s,
                    center_y,
                    s,
                    toolbar.sub_options.filled,
                    color,
                );
            }
            Some(SubControl::CornerRadius) => {
                render_corner_radius_with_value(
                    pixmap,
                    &entry.rect,
                    center_y,
                    s,
                    toolbar.sub_options.corner_radius,
                    toolbar.focused_sub_control == Some(SubControl::CornerRadius),
                );
            }
            Some(SubControl::ArrowStyle) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::Arrow);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_arrow_preview(
                    pixmap,
                    entry.rect.x + 8.0 * s,
                    entry.rect.x + entry.rect.width - 14.0 * s,
                    center_y,
                    s,
                    toolbar.sub_options.arrow_style,
                    Color::from_rgba8(45, 48, 56, 255),
                );
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 8.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::Color(index)) => {
                let Some(preset) = presets.get(index) else {
                    continue;
                };
                let is_selected = selected_color_index == index;
                let cx = entry.rect.x + 8.0 * s;
                fill_circle(pixmap, cx, center_y, 8.0 * s, *preset);
                stroke_circle(
                    pixmap,
                    cx,
                    center_y,
                    8.0 * s,
                    Color::from_rgba8(0, 0, 0, 32),
                    1.0 * s,
                );
                if is_selected {
                    stroke_circle(
                        pixmap,
                        cx,
                        center_y,
                        10.5 * s,
                        Color::from_rgba8(37, 99, 235, 255),
                        2.2 * s,
                    );
                }
            }
            Some(SubControl::FontSize) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::FontSize);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        &format!("A {}", toolbar.sub_options.font_size.round() as u32),
                        entry.rect.x + 7.0 * s,
                        entry.rect.y + 5.0 * s,
                        12.0 * s,
                        (45, 48, 56, 255),
                    );
                });
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 9.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::StartArrow) => {
                render_arrow_head_preview(
                    pixmap,
                    ArrowHeadPreviewSpec {
                        x0: entry.rect.x + 10.0 * s,
                        x1: entry.rect.x + entry.rect.width - 18.0 * s,
                        cy: center_y,
                        scale: s,
                        style: toolbar.sub_options.start_arrow_head,
                        color: Color::from_rgba8(45, 48, 56, 255),
                        at_start: true,
                    },
                );
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 9.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::EndArrow) => {
                render_arrow_head_preview(
                    pixmap,
                    ArrowHeadPreviewSpec {
                        x0: entry.rect.x + 10.0 * s,
                        x1: entry.rect.x + entry.rect.width - 18.0 * s,
                        cy: center_y,
                        scale: s,
                        style: toolbar.sub_options.end_arrow_head,
                        color: Color::from_rgba8(45, 48, 56, 255),
                        at_start: false,
                    },
                );
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 9.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::Bold) => {
                render_toggle_text(pixmap, &entry.rect, s, "B", toolbar.sub_options.text_bold)
            }
            Some(SubControl::Italic) => {
                render_toggle_text(pixmap, &entry.rect, s, "I", toolbar.sub_options.text_italic)
            }
            Some(SubControl::TextStroke) => render_toggle_text(
                pixmap,
                &entry.rect,
                s,
                "描边",
                toolbar.sub_options.text_stroke_width > 0.0,
            ),
            Some(SubControl::TextBackground) => render_toggle_text(
                pixmap,
                &entry.rect,
                s,
                "背景",
                toolbar.sub_options.text_background_enabled,
            ),
            Some(SubControl::MarkerShape) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::MarkerShape);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        snapin_core::i18n::tr(toolbar.sub_options.marker_shape.label()),
                        entry.rect.x + 7.0 * s,
                        entry.rect.y + 5.0 * s,
                        12.0 * s,
                        (45, 48, 56, 255),
                    );
                });
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 9.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::MarkerBlendMode) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::MarkerBlendMode);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        snapin_core::i18n::tr(toolbar.sub_options.marker_blend_mode.label()),
                        entry.rect.x + 7.0 * s,
                        entry.rect.y + 5.0 * s,
                        12.0 * s,
                        (45, 48, 56, 255),
                    );
                });
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 9.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::EraserMode) => render_toggle_text(
                pixmap,
                &entry.rect,
                s,
                if toolbar.sub_options.eraser_rectangle_mode {
                    "框选"
                } else {
                    "画笔"
                },
                true,
            ),
            Some(SubControl::ClearAnnotations) => {
                render_toggle_text(pixmap, &entry.rect, s, "清空", false)
            }
            Some(SubControl::LineMode) => render_toggle_text(
                pixmap,
                &entry.rect,
                s,
                toolbar.sub_options.line_mode.label(),
                true,
            ),
            Some(SubControl::ArrowEnabled) => render_toggle_text(
                pixmap,
                &entry.rect,
                s,
                "箭头",
                toolbar.sub_options.line_arrow_enabled,
            ),
            Some(SubControl::MosaicBrush) => render_toggle_text(
                pixmap,
                &entry.rect,
                s,
                "画笔",
                toolbar.sub_options.mosaic_brush_mode,
            ),
            Some(SubControl::MosaicRectangle) => render_toggle_text(
                pixmap,
                &entry.rect,
                s,
                "矩形",
                !toolbar.sub_options.mosaic_brush_mode,
            ),
            Some(SubControl::MosaicMode) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::MosaicMode);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        snapin_core::i18n::tr(toolbar.sub_options.mosaic_mode.label()),
                        entry.rect.x + 7.0 * s,
                        entry.rect.y + 5.0 * s,
                        12.0 * s,
                        (45, 48, 56, 255),
                    );
                });
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 9.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::FontFamily) => {
                let open = toolbar.open_dropdown == Some(SubDropdown::FontFamily);
                if open {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                let family = toolbar
                    .sub_options
                    .font_family
                    .map(snapin_core::i18n::tr)
                    .unwrap_or_else(|| snapin_core::i18n::tr("系统默认"));
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        family,
                        entry.rect.x + 7.0 * s,
                        entry.rect.y + 5.0 * s,
                        11.0 * s,
                        (45, 48, 56, 255),
                    );
                });
                render_caret(
                    pixmap,
                    entry.rect.x + entry.rect.width - 9.0 * s,
                    center_y,
                    s,
                );
            }
            Some(SubControl::TextBackgroundOpacity) => render_numeric(
                pixmap,
                &entry.rect,
                center_y,
                s,
                &format!("{}%", toolbar.sub_options.text_background_opacity),
                toolbar.sub_options.text_background_enabled,
            ),
            Some(SubControl::TextBackgroundPadding) => render_numeric(
                pixmap,
                &entry.rect,
                center_y,
                s,
                &format!(
                    "{} {:.0}",
                    snapin_core::i18n::tr("边距"),
                    toolbar.sub_options.text_background_padding
                ),
                toolbar.sub_options.text_background_enabled,
            ),
            Some(SubControl::TextBackgroundCornerRadius) => render_numeric(
                pixmap,
                &entry.rect,
                center_y,
                s,
                &format!(
                    "{} {:.0}",
                    snapin_core::i18n::tr("圆角"),
                    toolbar.sub_options.text_background_corner_radius
                ),
                toolbar.sub_options.text_background_enabled,
            ),
            Some(SubControl::SpotlightShapeRect) => {
                let active = toolbar.sub_options.spotlight_shape == SpotlightShape::Rectangle;
                if active {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_sub_shape_rect(pixmap, entry.rect.x + 12.0 * s, center_y, s, active);
            }
            Some(SubControl::SpotlightShapeEllipse) => {
                let active = toolbar.sub_options.spotlight_shape == SpotlightShape::Ellipse;
                if active {
                    fill_button_highlight(pixmap, &entry.rect, highlight, s);
                }
                render_sub_shape_ellipse(pixmap, entry.rect.x + 12.0 * s, center_y, s, active);
            }
            Some(SubControl::SpotlightOpacity) => render_numeric(
                pixmap,
                &entry.rect,
                center_y,
                s,
                &format!(
                    "{}%",
                    spotlight_opacity_percent(toolbar.sub_options.spotlight_intensity)
                ),
                true,
            ),
            Some(SubControl::SpotlightOutline) => render_toggle_text(
                pixmap,
                &entry.rect,
                s,
                "描边",
                toolbar.sub_options.spotlight_outline,
            ),
            Some(_) => {}
        }
    }

    if let Some(dropdown) = &layout.dropdown {
        render_dropdown(pixmap, dropdown, toolbar, s);
    }
}
