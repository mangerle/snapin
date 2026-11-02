//! 二级属性栏下拉菜单绘制。

use super::super::icons::{add_round_rect, make_paint};
use super::super::items::{MosaicMode, FONT_FAMILY_PRESETS, FONT_SIZE_PRESETS};
use super::super::layout::{DropdownLayout, SubDropdown};
use super::super::Toolbar;
use super::widgets::{
    fill_circle, render_arrow_head_preview, render_arrow_preview, render_badge_style_icon,
    render_magnifier_connector_preview, stroke_sample, ArrowHeadPreviewSpec,
};
use snapin_core::annotation::{
    ArrowStyle, BadgeStyle, LineStyle, MagnifierConnector, MarkerBlendMode, MarkerShape,
};
use snapin_core::render::with_thread_local_text_renderer;
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制展开的下拉菜单面板与选项预览
pub(super) fn render_dropdown(
    pixmap: &mut PixmapMut,
    dropdown: &DropdownLayout,
    toolbar: &Toolbar,
    s: f32,
) {
    let rect = dropdown.rect;

    // 柔影与白色圆角面板
    let mut shadow_pb = PathBuilder::new();
    add_round_rect(
        &mut shadow_pb,
        rect.x + 1.0 * s,
        rect.y + 2.0 * s,
        rect.width,
        rect.height,
        8.0 * s,
    );
    if let Some(path) = shadow_pb.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(0, 0, 0, 36)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let mut pb = PathBuilder::new();
    add_round_rect(&mut pb, rect.x, rect.y, rect.width, rect.height, 8.0 * s);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(255, 255, 255, 255)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
        let border = Stroke {
            width: 1.0 * s,
            ..Default::default()
        };
        pixmap.stroke_path(
            &path,
            &make_paint(Color::from_rgba8(218, 224, 233, 255)),
            &border,
            Transform::identity(),
            None,
        );
    }

    let ink = Color::from_rgba8(45, 48, 56, 255);
    for (index, row) in dropdown.rows.iter().enumerate() {
        let (is_current, preview) = match dropdown.kind {
            SubDropdown::Line => {
                let Some(style) = LineStyle::ALL.get(index) else {
                    continue;
                };
                (
                    toolbar.sub_options.line_style == *style,
                    DropdownPreview::Line(*style),
                )
            }
            SubDropdown::Badge => {
                let style = if index == 0 {
                    BadgeStyle::Filled
                } else {
                    BadgeStyle::Outline
                };
                (
                    toolbar.sub_options.badge_style == style,
                    DropdownPreview::Badge(style),
                )
            }
            SubDropdown::Arrow => {
                let Some(style) = ArrowStyle::ALL.get(index) else {
                    continue;
                };
                (
                    toolbar.sub_options.arrow_style == *style,
                    DropdownPreview::Arrow(*style),
                )
            }
            SubDropdown::FontSize => {
                let Some(font_size) = FONT_SIZE_PRESETS.get(index) else {
                    continue;
                };
                (
                    (toolbar.sub_options.font_size - *font_size).abs() < 0.5,
                    DropdownPreview::FontSize(*font_size),
                )
            }
            SubDropdown::StartArrow => {
                let Some(style) = snapin_core::annotation::style::ArrowHeadStyle::ALL.get(index)
                else {
                    continue;
                };
                (
                    toolbar.sub_options.start_arrow_head == *style,
                    DropdownPreview::ArrowHead(*style, true),
                )
            }
            SubDropdown::EndArrow => {
                let Some(style) = snapin_core::annotation::style::ArrowHeadStyle::ALL.get(index)
                else {
                    continue;
                };
                (
                    toolbar.sub_options.end_arrow_head == *style,
                    DropdownPreview::ArrowHead(*style, false),
                )
            }
            SubDropdown::BadgeSequence => {
                let Some(sequence) = snapin_core::annotation::BadgeSequence::ALL.get(index) else {
                    continue;
                };
                (
                    toolbar.sub_options.badge_sequence == *sequence,
                    DropdownPreview::BadgeSequence(*sequence),
                )
            }
            SubDropdown::MosaicMode => {
                let mode = match index {
                    0 => MosaicMode::Mosaic,
                    1 => MosaicMode::Blur,
                    2 => MosaicMode::SmartRedact,
                    _ => continue,
                };
                (
                    toolbar.sub_options.mosaic_mode == mode,
                    DropdownPreview::Text(mode.label()),
                )
            }
            SubDropdown::FontFamily => {
                let Some((label, family)) = FONT_FAMILY_PRESETS.get(index) else {
                    continue;
                };
                (
                    toolbar.sub_options.font_family == *family,
                    DropdownPreview::Text(label),
                )
            }
            SubDropdown::MagnifierConnector => {
                let Some(connector) = MagnifierConnector::ALL.get(index) else {
                    continue;
                };
                (
                    toolbar.sub_options.magnifier_connector == *connector,
                    DropdownPreview::MagnifierConnector(*connector),
                )
            }
            SubDropdown::MarkerShape => {
                let shape = match index {
                    0 => MarkerShape::Stroke,
                    1 => MarkerShape::Rectangle,
                    _ => continue,
                };
                (
                    toolbar.sub_options.marker_shape == shape,
                    DropdownPreview::Text(shape.label()),
                )
            }
            SubDropdown::MarkerBlendMode => {
                let mode = match index {
                    0 => MarkerBlendMode::Multiply,
                    1 => MarkerBlendMode::Normal,
                    _ => continue,
                };
                (
                    toolbar.sub_options.marker_blend_mode == mode,
                    DropdownPreview::Text(mode.label()),
                )
            }
        };

        if is_current {
            let mut row_pb = PathBuilder::new();
            add_round_rect(&mut row_pb, row.x, row.y, row.width, row.height, 5.0 * s);
            if let Some(path) = row_pb.finish() {
                pixmap.fill_path(
                    &path,
                    &make_paint(Color::from_rgba8(37, 99, 235, 28)),
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        } else if toolbar.dropdown_hover == Some(index) {
            let mut row_pb = PathBuilder::new();
            add_round_rect(&mut row_pb, row.x, row.y, row.width, row.height, 5.0 * s);
            if let Some(path) = row_pb.finish() {
                pixmap.fill_path(
                    &path,
                    &make_paint(Color::from_rgba8(148, 163, 184, 34)),
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        let center_y = row.y + row.height / 2.0;
        match preview {
            DropdownPreview::Line(style) => stroke_sample(
                pixmap,
                row.x + 10.0 * s,
                row.x + row.width - 24.0 * s,
                center_y,
                2.0 * s,
                style,
                ink,
            ),
            DropdownPreview::Badge(style) => {
                render_badge_style_icon(pixmap, row.x + 18.0 * s, center_y, s, style, ink)
            }
            DropdownPreview::Arrow(style) => render_arrow_preview(
                pixmap,
                row.x + 10.0 * s,
                row.x + row.width - 24.0 * s,
                center_y,
                s,
                style,
                ink,
            ),
            DropdownPreview::FontSize(font_size) => {
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        &format!("{font_size:.0} px"),
                        row.x + 12.0 * s,
                        center_y - 8.0 * s,
                        14.0 * s,
                        (45, 48, 56, 255),
                    );
                });
            }
            DropdownPreview::ArrowHead(style, at_start) => {
                render_arrow_head_preview(
                    pixmap,
                    ArrowHeadPreviewSpec {
                        x0: row.x + 12.0 * s,
                        x1: row.x + row.width - 24.0 * s,
                        cy: center_y,
                        scale: s,
                        style,
                        color: ink,
                        at_start,
                    },
                );
            }
            DropdownPreview::BadgeSequence(sequence) => {
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        &format!("{}  {}", sequence.label(1), sequence.label(2)),
                        row.x + 12.0 * s,
                        center_y - 8.0 * s,
                        14.0 * s,
                        (45, 48, 56, 255),
                    );
                });
            }
            DropdownPreview::MagnifierConnector(connector) => {
                render_magnifier_connector_preview(
                    pixmap,
                    row.x + 12.0 * s,
                    row.x + row.width - 26.0 * s,
                    center_y,
                    s,
                    connector,
                    ink,
                );
            }
            DropdownPreview::Text(label) => {
                with_thread_local_text_renderer(|renderer| {
                    renderer.draw_text_rgba(
                        pixmap,
                        snapin_core::i18n::tr(label),
                        row.x + 12.0 * s,
                        center_y - 8.0 * s,
                        14.0 * s,
                        (45, 48, 56, 255),
                    );
                });
            }
        }

        if is_current {
            fill_circle(
                pixmap,
                row.x + row.width - 12.0 * s,
                center_y,
                3.0 * s,
                Color::from_rgba8(37, 99, 235, 255),
            );
        }
    }
}

/// 下拉行的预览类型
enum DropdownPreview {
    Line(LineStyle),
    Badge(BadgeStyle),
    Arrow(ArrowStyle),
    FontSize(f32),
    ArrowHead(snapin_core::annotation::style::ArrowHeadStyle, bool),
    BadgeSequence(snapin_core::annotation::BadgeSequence),
    MagnifierConnector(MagnifierConnector),
    Text(&'static str),
}
