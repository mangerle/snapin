//! 箭头样式与端点预览绘制。

use super::stroke_sample;
use crate::editor::toolbar::icons::make_paint;
use snapin_annotation::{ArrowHeadStyle, ArrowStyle};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制迷你箭头预览，三种形态同尺寸对比
pub(crate) fn render_arrow_preview(
    pixmap: &mut PixmapMut,
    x0: f32,
    x1: f32,
    cy: f32,
    s: f32,
    style: ArrowStyle,
    color: Color,
) {
    let head_len = 8.0 * s;
    let head_half = 4.0 * s;
    let shaft_half = 1.2 * s;
    let base_x = x1 - head_len;

    match style.shaft_line_style() {
        Some(line_style) => {
            stroke_sample(pixmap, x0, base_x, cy, 2.0 * s, line_style, color);
        }
        None => {
            let tail_half = shaft_half * 0.32;
            let mut pb = PathBuilder::new();
            pb.move_to(x0, cy - tail_half);
            pb.line_to(base_x, cy - shaft_half);
            pb.line_to(base_x, cy - head_half);
            pb.line_to(x1, cy);
            pb.line_to(base_x, cy + head_half);
            pb.line_to(base_x, cy + shaft_half);
            pb.line_to(x0, cy + tail_half);
            pb.close();
            if let Some(path) = pb.finish() {
                pixmap.fill_path(
                    &path,
                    &make_paint(color),
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
            return;
        }
    }

    let mut head = PathBuilder::new();
    head.move_to(x1, cy);
    head.line_to(base_x, cy - head_half);
    head.line_to(base_x, cy + head_half);
    head.close();
    if let Some(path) = head.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 箭头端点预览参数；集中几何与样式，避免调用点出现过长参数列表。
#[derive(Clone, Copy)]
pub(crate) struct ArrowHeadPreviewSpec {
    pub x0: f32,
    pub x1: f32,
    pub cy: f32,
    pub scale: f32,
    pub style: ArrowHeadStyle,
    pub color: Color,
    pub at_start: bool,
}

/// 绘制箭头端点图案；`at_start` 为真时箭头位于线段左端。
pub(crate) fn render_arrow_head_preview(pixmap: &mut PixmapMut, preview: ArrowHeadPreviewSpec) {
    let x0 = preview.x0;
    let x1 = preview.x1;
    let cy = preview.cy;
    let s = preview.scale;
    let style = preview.style;
    let at_start = preview.at_start;
    draw_arrow_shaft(pixmap, x0, x1, cy, s, preview.color);
    if style == ArrowHeadStyle::None {
        return;
    }
    let tip_x = if at_start { x0 } else { x1 };
    let direction = if at_start { 1.0 } else { -1.0 };
    let geometry = ArrowHeadGeometry {
        tip_x,
        base_x: tip_x + direction * 8.0 * s,
        half: 4.0 * s,
    };
    match style {
        ArrowHeadStyle::Triangle | ArrowHeadStyle::Outline => {
            draw_triangle_head(pixmap, preview, geometry)
        }
        ArrowHeadStyle::Bar => draw_bar_head(pixmap, preview, geometry),
        ArrowHeadStyle::Circle => draw_circle_head(pixmap, preview, geometry),
        ArrowHeadStyle::None => {}
    }
}

/// 箭头端点的几何位置。
#[derive(Clone, Copy)]
struct ArrowHeadGeometry {
    tip_x: f32,
    base_x: f32,
    half: f32,
}

/// 绘制箭头预览的水平杆。
fn draw_arrow_shaft(pixmap: &mut PixmapMut, x0: f32, x1: f32, cy: f32, s: f32, color: Color) {
    let mut shaft = PathBuilder::new();
    shaft.move_to(x0, cy);
    shaft.line_to(x1, cy);
    if let Some(path) = shaft.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(color),
            &Stroke {
                width: 1.8 * s,
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// 绘制实心或空心三角箭头。
fn draw_triangle_head(
    pixmap: &mut PixmapMut,
    preview: ArrowHeadPreviewSpec,
    geometry: ArrowHeadGeometry,
) {
    let mut head = PathBuilder::new();
    head.move_to(geometry.tip_x, preview.cy);
    head.line_to(geometry.base_x, preview.cy - geometry.half);
    head.line_to(geometry.base_x, preview.cy + geometry.half);
    head.close();
    let Some(path) = head.finish() else {
        return;
    };
    if preview.style == ArrowHeadStyle::Triangle {
        pixmap.fill_path(
            &path,
            &make_paint(preview.color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    } else {
        pixmap.stroke_path(
            &path,
            &make_paint(preview.color),
            &Stroke {
                width: 1.5 * preview.scale,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// 绘制竖线端点。
fn draw_bar_head(
    pixmap: &mut PixmapMut,
    preview: ArrowHeadPreviewSpec,
    geometry: ArrowHeadGeometry,
) {
    let mut head = PathBuilder::new();
    head.move_to(geometry.tip_x, preview.cy - geometry.half);
    head.line_to(geometry.tip_x, preview.cy + geometry.half);
    if let Some(path) = head.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(preview.color),
            &Stroke {
                width: 2.0 * preview.scale,
                line_cap: tiny_skia::LineCap::Round,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// 绘制圆点端点。
fn draw_circle_head(
    pixmap: &mut PixmapMut,
    preview: ArrowHeadPreviewSpec,
    geometry: ArrowHeadGeometry,
) {
    let mut head = PathBuilder::new();
    head.push_circle(geometry.tip_x, preview.cy, geometry.half * 0.75);
    if let Some(path) = head.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(preview.color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}
