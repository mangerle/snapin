//! 箭头样式与端点预览绘制。

use super::{stroke_sample, LineSampleSpec, PreviewSpan};
use crate::editor::toolbar::icons::make_paint;
use snapin_annotation::{ArrowHeadStyle, ArrowStyle};
use tiny_skia::{PathBuilder, PixmapMut, Stroke, Transform};

/// 绘制迷你箭头预览，三种形态同尺寸对比
pub(crate) fn render_arrow_preview(pixmap: &mut PixmapMut, spec: ArrowPreviewSpec) {
    let geometry = ArrowPreviewGeometry::new(spec.span);
    match spec.style.shaft_line_style() {
        Some(line_style) => {
            stroke_sample(
                pixmap,
                LineSampleSpec {
                    x0: geometry.span.x0,
                    x1: geometry.base_x,
                    y: geometry.span.center_y,
                    width: 2.0 * geometry.span.scale,
                    style: line_style,
                    color: geometry.span.color,
                },
            );
        }
        None => {
            draw_filled_arrow(pixmap, geometry);
            return;
        }
    }
    draw_filled_arrow_head(pixmap, geometry);
}

/// 箭头预览的派生几何，避免各绘制阶段重复计算端点。
#[derive(Clone, Copy)]
struct ArrowPreviewGeometry {
    span: PreviewSpan,
    base_x: f32,
    head_half: f32,
    shaft_half: f32,
}

impl ArrowPreviewGeometry {
    fn new(span: PreviewSpan) -> Self {
        Self {
            span,
            base_x: span.x1 - 8.0 * span.scale,
            head_half: 4.0 * span.scale,
            shaft_half: 1.2 * span.scale,
        }
    }
}

/// 绘制实心箭头杆与箭头。
fn draw_filled_arrow(pixmap: &mut PixmapMut, geometry: ArrowPreviewGeometry) {
    let span = geometry.span;
    let tail_half = geometry.shaft_half * 0.32;
    let mut path = PathBuilder::new();
    path.move_to(span.x0, span.center_y - tail_half);
    path.line_to(geometry.base_x, span.center_y - geometry.shaft_half);
    path.line_to(geometry.base_x, span.center_y - geometry.head_half);
    path.line_to(span.x1, span.center_y);
    path.line_to(geometry.base_x, span.center_y + geometry.head_half);
    path.line_to(geometry.base_x, span.center_y + geometry.shaft_half);
    path.line_to(span.x0, span.center_y + tail_half);
    path.close();
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(span.color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制线型箭头尾部的三角箭头。
fn draw_filled_arrow_head(pixmap: &mut PixmapMut, geometry: ArrowPreviewGeometry) {
    let span = geometry.span;
    let mut path = PathBuilder::new();
    path.move_to(span.x1, span.center_y);
    path.line_to(geometry.base_x, span.center_y - geometry.head_half);
    path.line_to(geometry.base_x, span.center_y + geometry.head_half);
    path.close();
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(span.color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 箭头预览的共享几何、缩放下与颜色。
#[derive(Clone, Copy)]
pub(crate) struct ArrowPreviewSpec {
    pub span: PreviewSpan,
    pub style: ArrowStyle,
}

/// 箭头端点预览参数；集中几何与样式，避免调用点出现过长参数列表。
#[derive(Clone, Copy)]
pub(crate) struct ArrowHeadPreviewSpec {
    pub span: PreviewSpan,
    pub style: ArrowHeadStyle,
    pub at_start: bool,
}

/// 绘制箭头端点图案；`at_start` 为真时箭头位于线段左端。
pub(crate) fn render_arrow_head_preview(pixmap: &mut PixmapMut, preview: ArrowHeadPreviewSpec) {
    let span = preview.span;
    let style = preview.style;
    let at_start = preview.at_start;
    draw_arrow_shaft(pixmap, span);
    if style == ArrowHeadStyle::None {
        return;
    }
    let tip_x = if at_start { span.x0 } else { span.x1 };
    let direction = if at_start { 1.0 } else { -1.0 };
    let geometry = ArrowHeadGeometry {
        tip_x,
        base_x: tip_x + direction * 8.0 * span.scale,
        half: 4.0 * span.scale,
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
fn draw_arrow_shaft(pixmap: &mut PixmapMut, span: PreviewSpan) {
    let mut shaft = PathBuilder::new();
    shaft.move_to(span.x0, span.center_y);
    shaft.line_to(span.x1, span.center_y);
    if let Some(path) = shaft.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(span.color),
            &Stroke {
                width: 1.8 * span.scale,
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
    head.move_to(geometry.tip_x, preview.span.center_y);
    head.line_to(geometry.base_x, preview.span.center_y - geometry.half);
    head.line_to(geometry.base_x, preview.span.center_y + geometry.half);
    head.close();
    let Some(path) = head.finish() else {
        return;
    };
    if preview.style == ArrowHeadStyle::Triangle {
        pixmap.fill_path(
            &path,
            &make_paint(preview.span.color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    } else {
        pixmap.stroke_path(
            &path,
            &make_paint(preview.span.color),
            &Stroke {
                width: 1.5 * preview.span.scale,
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
    head.move_to(geometry.tip_x, preview.span.center_y - geometry.half);
    head.line_to(geometry.tip_x, preview.span.center_y + geometry.half);
    if let Some(path) = head.finish() {
        pixmap.stroke_path(
            &path,
            &make_paint(preview.span.color),
            &Stroke {
                width: 2.0 * preview.span.scale,
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
    head.push_circle(geometry.tip_x, preview.span.center_y, geometry.half * 0.75);
    if let Some(path) = head.finish() {
        pixmap.fill_path(
            &path,
            &make_paint(preview.span.color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}
