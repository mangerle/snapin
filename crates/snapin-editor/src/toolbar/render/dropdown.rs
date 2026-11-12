//! 二级属性栏下拉菜单绘制。

use super::super::icons::{add_round_rect, make_paint, RoundRect};
use super::super::items::{MosaicMode, FONT_FAMILY_PRESETS, FONT_SIZE_PRESETS};
use super::super::layout::{DropdownLayout, SubDropdown};
use super::super::Toolbar;
use super::widgets::{
    fill_circle, render_arrow_head_preview, render_arrow_preview, render_badge_style_icon,
    render_magnifier_connector_preview, stroke_sample, ArrowHeadPreviewSpec, ArrowPreviewSpec,
    BadgeStyleIconSpec, CircleSpec, LineSampleSpec, MagnifierConnectorPreviewSpec, PreviewSpan,
};
use crate::geometry::Rect;
use snapin_annotation::{
    ArrowStyle, BadgeStyle, LineStyle, MagnifierConnector, MarkerBlendMode, MarkerShape,
};
use snapin_render::{with_thread_local_text_renderer, TextDrawOptions};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 下拉菜单单行的几何与文字颜色。
#[derive(Clone, Copy)]
struct DropdownRowContext {
    rect: Rect,
    center_y: f32,
    scale: f32,
    ink: Color,
}

/// 下拉行背景的当前选中、悬停与缩放状态。
#[derive(Clone, Copy)]
struct DropdownRowBackground {
    context: DropdownRowContext,
    current: bool,
    hovered: bool,
}

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
        let center_y = row.y + row.height / 2.0;
        let context = DropdownRowContext {
            rect: *row,
            center_y,
            scale: s,
            ink,
        };
        render_row_background(
            pixmap,
            DropdownRowBackground {
                context,
                current: is_current,
                hovered: toolbar.dropdown_hover == Some(index),
            },
        );
        render_preview(pixmap, context, preview);
        if is_current {
            fill_circle(
                pixmap,
                CircleSpec {
                    center_x: row.x + row.width - 12.0 * s,
                    center_y,
                    radius: 3.0 * s,
                    color: Color::from_rgba8(37, 99, 235, 255),
                },
            );
        }
    }
}

/// 绘制下拉面板的柔影、白色底板与边框。
fn render_panel(pixmap: &mut PixmapMut, rect: Rect, s: f32) {
    draw_dropdown_shadow(pixmap, rect, s);
    draw_dropdown_body(pixmap, rect, s);
}

/// 绘制下拉面板的柔影。
fn draw_dropdown_shadow(pixmap: &mut PixmapMut, rect: Rect, s: f32) {
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: rect.x + 1.0 * s,
            y: rect.y + 2.0 * s,
            width: rect.width,
            height: rect.height,
            radius: 8.0 * s,
        },
    );
    if let Some(path) = builder.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(Color::from_rgba8(0, 0, 0, 36)),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制下拉面板的白色底板与边框。
fn draw_dropdown_body(pixmap: &mut PixmapMut, rect: Rect, s: f32) {
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            radius: 8.0 * s,
        },
    );
    let Some(path) = builder.finish() else {
        return;
    };
    pixmap.fill_path(
        &path,
        &make_paint(Color::from_rgba8(255, 255, 255, 255)),
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    pixmap.stroke_path(
        &path,
        &make_paint(Color::from_rgba8(218, 224, 233, 255)),
        &Stroke {
            width: 1.0 * s,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
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
                2 if toolbar.smart_redact_enabled => MosaicMode::SmartRedact,
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
fn render_row_background(pixmap: &mut PixmapMut, spec: DropdownRowBackground) {
    let color = if spec.current {
        Color::from_rgba8(37, 99, 235, 28)
    } else if spec.hovered {
        Color::from_rgba8(148, 163, 184, 34)
    } else {
        return;
    };
    let row = spec.context.rect;
    let mut builder = PathBuilder::new();
    add_round_rect(
        &mut builder,
        RoundRect {
            x: row.x,
            y: row.y,
            width: row.width,
            height: row.height,
            radius: 5.0 * spec.context.scale,
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
fn render_preview(pixmap: &mut PixmapMut, context: DropdownRowContext, preview: DropdownPreview) {
    let row = context.rect;
    let center_y = context.center_y;
    let s = context.scale;
    let ink = context.ink;
    match preview {
        DropdownPreview::Line(style) => render_line_preview(pixmap, context, style),
        DropdownPreview::Badge(style) => {
            render_badge_style_icon(
                pixmap,
                BadgeStyleIconSpec {
                    center_x: row.x + 18.0 * s,
                    center_y,
                    scale: s,
                    style,
                    color: ink,
                },
            );
        }
        DropdownPreview::Arrow(style) => {
            render_arrow_preview(
                pixmap,
                ArrowPreviewSpec {
                    span: PreviewSpan {
                        x0: row.x + 10.0 * s,
                        x1: row.x + row.width - 24.0 * s,
                        center_y,
                        scale: s,
                        color: ink,
                    },
                    style,
                },
            );
        }
        DropdownPreview::ArrowHead(style, at_start) => render_arrow_head_preview(
            pixmap,
            ArrowHeadPreviewSpec {
                span: PreviewSpan {
                    x0: row.x + 12.0 * s,
                    x1: row.x + row.width - 24.0 * s,
                    center_y,
                    scale: s,
                    color: ink,
                },
                style,
                at_start,
            },
        ),
        DropdownPreview::MagnifierConnector(connector) => {
            render_magnifier_connector_preview(
                pixmap,
                MagnifierConnectorPreviewSpec {
                    span: PreviewSpan {
                        x0: row.x + 12.0 * s,
                        x1: row.x + row.width - 26.0 * s,
                        center_y,
                        scale: s,
                        color: ink,
                    },
                    connector,
                },
            );
        }
        DropdownPreview::FontSize(font_size) => {
            render_text_preview(pixmap, context, &format!("{font_size:.0} px"));
        }
        DropdownPreview::BadgeSequence(sequence) => render_text_preview(
            pixmap,
            context,
            &format!("{}  {}", sequence.label(1), sequence.label(2)),
        ),
        DropdownPreview::Text(label) => {
            render_text_preview(pixmap, context, snapin_foundation::i18n::tr(label));
        }
    }
}

/// 绘制线型示例。
fn render_line_preview(pixmap: &mut PixmapMut, context: DropdownRowContext, style: LineStyle) {
    let row = context.rect;
    let scale = context.scale;
    stroke_sample(
        pixmap,
        LineSampleSpec {
            x0: row.x + 10.0 * scale,
            x1: row.x + row.width - 24.0 * scale,
            y: context.center_y,
            width: 2.0 * scale,
            style,
            color: context.ink,
        },
    );
}

/// 绘制文本型预览。
fn render_text_preview(pixmap: &mut PixmapMut, context: DropdownRowContext, label: &str) {
    let ink = context.ink;
    let rgba = (
        (ink.red() * 255.0).round() as u8,
        (ink.green() * 255.0).round() as u8,
        (ink.blue() * 255.0).round() as u8,
        (ink.alpha() * 255.0).round() as u8,
    );
    with_thread_local_text_renderer(|renderer| {
        renderer.draw_text_rgba(
            pixmap,
            label,
            TextDrawOptions::new(
                context.rect.x + 12.0 * context.scale,
                context.center_y - 8.0 * context.scale,
                14.0 * context.scale,
                rgba,
            ),
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
