//! 二级属性栏下拉菜单绘制。

use super::super::icons::{add_round_rect, make_paint, RoundRect};
use super::super::items::{MosaicMode, FONT_FAMILY_PRESETS, FONT_SIZE_PRESETS};
use super::super::layout::{DropdownLayout, SubDropdown};
use super::super::Toolbar;
use super::widgets::{
    fill_circle, render_arrow_head_preview, render_arrow_preview, render_badge_style_icon,
    render_magnifier_connector_preview, stroke_sample, ArrowHeadPreviewSpec,
    MagnifierConnectorPreviewSpec,
};
use crate::geometry::Rect;
use snapin_annotation::{
    ArrowStyle, BadgeStyle, LineStyle, MagnifierConnector, MarkerBlendMode, MarkerShape,
};
use snapin_render::with_thread_local_text_renderer;
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制展开的下拉菜单面板与选项预览
pub(super) fn render_dropdown(
    pixmap: &mut PixmapMut,
    dropdown: &DropdownLayout,
    toolbar: &Toolbar,
    s: f32,
) {
    render_panel(pixmap, dropdown.rect, s);

    let ink = Color::from_rgba8(45, 48, 56, 255);
    for (index, row) in dropdown.rows.iter().enumerate() {
        let Some((is_current, preview)) = preview_for_row(dropdown.kind, index, toolbar) else {
            continue;
        };
        render_row_background(
            pixmap,
            *row,
            s,
            is_current,
            toolbar.dropdown_hover == Some(index),
        );
        let center_y = row.y + row.height / 2.0;
        render_preview(pixmap, *row, center_y, s, ink, preview);
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

/// 绘制下拉面板的柔影、白色底板与边框。
fn render_panel(pixmap: &mut PixmapMut, rect: Rect, s: f32) {
    let mut shadow_pb = PathBuilder::new();
    add_round_rect(
        &mut shadow_pb,
        RoundRect {
            x: rect.x + 1.0 * s,
            y: rect.y + 2.0 * s,
            width: rect.width,
            height: rect.height,
            radius: 8.0 * s,
        },
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
    add_round_rect(
        &mut pb,
        RoundRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            radius: 8.0 * s,
        },
    );
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
}

/// 解析当前行对应的选项状态与预览类型。
fn preview_for_row(
    kind: SubDropdown,
    index: usize,
    toolbar: &Toolbar,
) -> Option<(bool, DropdownPreview)> {
    let result = match kind {
        SubDropdown::Line => {
            let style = *LineStyle::ALL.get(index)?;
            (
                toolbar.sub_options.line_style == style,
                DropdownPreview::Line(style),
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
            let style = *ArrowStyle::ALL.get(index)?;
            (
                toolbar.sub_options.arrow_style == style,
                DropdownPreview::Arrow(style),
            )
        }
        SubDropdown::FontSize => {
            let font_size = *FONT_SIZE_PRESETS.get(index)?;
            (
                (toolbar.sub_options.font_size - font_size).abs() < 0.5,
                DropdownPreview::FontSize(font_size),
            )
        }
        SubDropdown::StartArrow => {
            let style = *snapin_annotation::style::ArrowHeadStyle::ALL.get(index)?;
            (
                toolbar.sub_options.start_arrow_head == style,
                DropdownPreview::ArrowHead(style, true),
            )
        }
        SubDropdown::EndArrow => {
            let style = *snapin_annotation::style::ArrowHeadStyle::ALL.get(index)?;
            (
                toolbar.sub_options.end_arrow_head == style,
                DropdownPreview::ArrowHead(style, false),
            )
        }
        SubDropdown::BadgeSequence => {
            let sequence = *snapin_annotation::BadgeSequence::ALL.get(index)?;
            (
                toolbar.sub_options.badge_sequence == sequence,
                DropdownPreview::BadgeSequence(sequence),
            )
        }
        SubDropdown::MosaicMode => {
            let mode = match index {
                0 => MosaicMode::Mosaic,
                1 => MosaicMode::Blur,
                2 => MosaicMode::SmartRedact,
                _ => return None,
            };
            (
                toolbar.sub_options.mosaic_mode == mode,
                DropdownPreview::Text(mode.label()),
            )
        }
        SubDropdown::FontFamily => {
            let (label, family) = FONT_FAMILY_PRESETS.get(index)?;
            (
                toolbar.sub_options.font_family == *family,
                DropdownPreview::Text(label),
            )
        }
        SubDropdown::MagnifierConnector => {
            let connector = *MagnifierConnector::ALL.get(index)?;
            (
                toolbar.sub_options.magnifier_connector == connector,
                DropdownPreview::MagnifierConnector(connector),
            )
        }
        SubDropdown::MarkerShape => {
            let shape = match index {
                0 => MarkerShape::Stroke,
                1 => MarkerShape::Rectangle,
                _ => return None,
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
                _ => return None,
            };
            (
                toolbar.sub_options.marker_blend_mode == mode,
                DropdownPreview::Text(mode.label()),
            )
        }
    };
    Some(result)
}

/// 绘制当前选中或悬停行的底色。
fn render_row_background(pixmap: &mut PixmapMut, row: Rect, s: f32, current: bool, hovered: bool) {
    let color = if current {
        Color::from_rgba8(37, 99, 235, 28)
    } else if hovered {
        Color::from_rgba8(148, 163, 184, 34)
    } else {
        return;
    };
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: row.x,
            y: row.y,
            width: row.width,
            height: row.height,
            radius: 5.0 * s,
        },
    );
    if let Some(path) = builder.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 按预览类别分派到图形或文本绘制。
fn render_preview(
    pixmap: &mut PixmapMut,
    row: Rect,
    center_y: f32,
    s: f32,
    ink: Color,
    preview: DropdownPreview,
) {
    match preview {
        DropdownPreview::Line(style) => render_line_preview(pixmap, row, center_y, s, ink, style),
        DropdownPreview::Badge(style) => {
            render_badge_style_icon(pixmap, row.x + 18.0 * s, center_y, s, style, ink);
        }
        DropdownPreview::Arrow(style) => {
            render_arrow_preview(
                pixmap,
                row.x + 10.0 * s,
                row.x + row.width - 24.0 * s,
                center_y,
                s,
                style,
                ink,
            );
        }
        DropdownPreview::ArrowHead(style, at_start) => render_arrow_head_preview(
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
        ),
        DropdownPreview::MagnifierConnector(connector) => {
            render_magnifier_connector_preview(
                pixmap,
                MagnifierConnectorPreviewSpec {
                    x0: row.x + 12.0 * s,
                    x1: row.x + row.width - 26.0 * s,
                    center_y,
                    scale: s,
                    connector,
                    color: ink,
                },
            );
        }
        DropdownPreview::FontSize(font_size) => {
            render_text_preview(pixmap, row, center_y, s, &format!("{font_size:.0} px"));
        }
        DropdownPreview::BadgeSequence(sequence) => render_text_preview(
            pixmap,
            row,
            center_y,
            s,
            &format!("{}  {}", sequence.label(1), sequence.label(2)),
        ),
        DropdownPreview::Text(label) => {
            render_text_preview(pixmap, row, center_y, s, snapin_foundation::i18n::tr(label));
        }
    }
}

/// 绘制线型示例。
fn render_line_preview(
    pixmap: &mut PixmapMut,
    row: Rect,
    center_y: f32,
    s: f32,
    ink: Color,
    style: LineStyle,
) {
    stroke_sample(
        pixmap,
        row.x + 10.0 * s,
        row.x + row.width - 24.0 * s,
        center_y,
        2.0 * s,
        style,
        ink,
    );
}

/// 绘制文本型预览。
fn render_text_preview(pixmap: &mut PixmapMut, row: Rect, center_y: f32, s: f32, label: &str) {
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            label,
            row.x + 12.0 * s,
            center_y - 8.0 * s,
            14.0 * s,
            (45, 48, 56, 255),
        );
    });
}

/// 下拉行的预览类型
enum DropdownPreview {
    Line(LineStyle),
    Badge(BadgeStyle),
    Arrow(ArrowStyle),
    FontSize(f32),
    ArrowHead(snapin_annotation::style::ArrowHeadStyle, bool),
    BadgeSequence(snapin_annotation::BadgeSequence),
    MagnifierConnector(MagnifierConnector),
    Text(&'static str),
}
