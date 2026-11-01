//! 二级属性栏下拉菜单绘制。

use super::super::icons::{add_round_rect, make_paint};
use super::super::items::CORNER_RADIUS_PRESETS;
use super::super::layout::{DropdownLayout, SubDropdown};
use super::super::Toolbar;
use super::widgets::{
    fill_circle, render_arrow_preview, render_badge_style_icon, render_corner_radius_icon,
    stroke_sample,
};
use snapin_core::annotation::{ArrowStyle, BadgeStyle, LineStyle};
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
            SubDropdown::Corner => {
                let Some(radius) = CORNER_RADIUS_PRESETS.get(index) else {
                    continue;
                };
                (
                    (toolbar.sub_options.corner_radius - *radius).abs() < 0.5,
                    DropdownPreview::Corner(*radius),
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
            DropdownPreview::Corner(radius) => {
                render_corner_radius_icon(pixmap, row.x + 18.0 * s, center_y, s, radius)
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
    Corner(f32),
}
