//! 马赛克、模糊与智能打码图标绘制。

use super::super::super::icons::make_paint;
use super::super::super::items::MosaicMode;
use super::common::{push_rect, push_rounded_rect, stroke_builder, IconRect, SubBarIconSpec};
use super::icons::render_marker_shape_icon;
use snapin_annotation::MarkerShape;
use tiny_skia::{PathBuilder, PixmapMut, Transform};

/// 绘制马赛克、模糊或智能打码模式图标。
pub(in crate::toolbar::render) fn render_mosaic_mode_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    mode: MosaicMode,
) {
    match mode {
        MosaicMode::Mosaic => render_mosaic_grid_icon(pixmap, spec),
        MosaicMode::Blur => render_blur_mode_icon(pixmap, spec),
        MosaicMode::SmartRedact => render_smart_redact_icon(pixmap, spec),
    }
}

fn render_mosaic_grid_icon(pixmap: &mut PixmapMut, spec: SubBarIconSpec) {
    let mut frame = PathBuilder::new();
    push_rounded_rect(
        &mut frame,
        spec,
        IconRect {
            x: -7.0,
            y: -7.0,
            width: 14.0,
            height: 14.0,
        },
        1.5,
    );
    stroke_builder(pixmap, frame, spec, 1.4);
    for (dx, dy) in [(-3.5, -3.5), (3.5, 3.5)] {
        let mut square = PathBuilder::new();
        push_rect(
            &mut square,
            spec,
            IconRect {
                x: dx - 2.5,
                y: dy - 2.5,
                width: 5.0,
                height: 5.0,
            },
        );
        if let Some(path) = square.finish() {
            pixmap.fill_path(
                &path,
                &make_paint(spec.color),
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

fn render_blur_mode_icon(pixmap: &mut PixmapMut, spec: SubBarIconSpec) {
    let mut builder = PathBuilder::new();
    let (top_x, top_y) = spec.point(0.0, -7.5);
    builder.move_to(top_x, top_y);
    builder.cubic_to(
        spec.center_x + 6.0 * spec.scale,
        spec.center_y - 1.0 * spec.scale,
        spec.center_x + 5.5 * spec.scale,
        spec.center_y + 6.5 * spec.scale,
        spec.center_x,
        spec.center_y + 6.5 * spec.scale,
    );
    builder.cubic_to(
        spec.center_x - 5.5 * spec.scale,
        spec.center_y + 6.5 * spec.scale,
        spec.center_x - 6.0 * spec.scale,
        spec.center_y - 1.0 * spec.scale,
        top_x,
        top_y,
    );
    builder.close();
    stroke_builder(pixmap, builder, spec, 1.5);
}

fn render_smart_redact_icon(pixmap: &mut PixmapMut, spec: SubBarIconSpec) {
    let mut frame = PathBuilder::new();
    push_rounded_rect(
        &mut frame,
        spec,
        IconRect {
            x: -7.0,
            y: -5.0,
            width: 14.0,
            height: 10.0,
        },
        2.0,
    );
    stroke_builder(pixmap, frame, spec, 1.4);
    let mut slash = PathBuilder::new();
    let (x0, y0) = spec.point(-7.5, 6.5);
    let (x1, y1) = spec.point(7.5, -6.5);
    slash.move_to(x0, y0);
    slash.line_to(x1, y1);
    stroke_builder(pixmap, slash, spec, 1.7);
}

/// 绘制马赛克画笔或矩形选择图标。
pub(in crate::toolbar::render) fn render_mosaic_shape_icon(
    pixmap: &mut PixmapMut,
    spec: SubBarIconSpec,
    brush: bool,
) {
    if brush {
        render_marker_shape_icon(pixmap, spec, MarkerShape::Stroke);
    } else {
        render_marker_shape_icon(pixmap, spec, MarkerShape::Rectangle);
    }
}
