//! 二级栏图标共享的数据契约与路径原语。

use super::super::super::icons::{add_round_rect, make_paint, RoundRect};
use tiny_skia::{Color, PathBuilder, PixmapMut, Stroke, Transform};

/// 二级栏图标的中心、缩放与颜色。
#[derive(Clone, Copy)]
pub(in crate::toolbar::render) struct SubBarIconSpec {
    pub(in crate::toolbar::render) center_x: f32,
    pub(in crate::toolbar::render) center_y: f32,
    pub(in crate::toolbar::render) scale: f32,
    pub(in crate::toolbar::render) color: Color,
}

/// 图标内部的逻辑矩形（相对图标中心）。
#[derive(Clone, Copy)]
pub(super) struct IconRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl SubBarIconSpec {
    pub(super) fn point(self, dx: f32, dy: f32) -> (f32, f32) {
        (
            self.center_x + dx * self.scale,
            self.center_y + dy * self.scale,
        )
    }
}

pub(super) fn stroke_builder(
    pixmap: &mut PixmapMut,
    builder: PathBuilder,
    spec: SubBarIconSpec,
    width: f32,
) {
    let Some(path) = builder.finish() else {
        return;
    };
    pixmap.stroke_path(
        &path,
        &make_paint(spec.color),
        &Stroke {
            width: width * spec.scale,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

pub(super) fn render_rectangle_icon(pixmap: &mut PixmapMut, spec: SubBarIconSpec, dashed: bool) {
    let mut builder = PathBuilder::new();
    push_rounded_rect(
        &mut builder,
        spec,
        IconRect {
            x: -7.0,
            y: -5.5,
            width: 14.0,
            height: 11.0,
        },
        2.0,
    );
    let Some(path) = builder.finish() else {
        return;
    };
    let mut stroke = Stroke {
        width: 1.5 * spec.scale,
        ..Default::default()
    };
    if dashed {
        stroke.dash = tiny_skia::StrokeDash::new(vec![3.0 * spec.scale, 2.0 * spec.scale], 0.0);
    }
    pixmap.stroke_path(
        &path,
        &make_paint(spec.color),
        &stroke,
        Transform::identity(),
        None,
    );
}

pub(super) fn push_rounded_rect(
    builder: &mut PathBuilder,
    spec: SubBarIconSpec,
    rect: IconRect,
    radius: f32,
) {
    let (x, y) = spec.point(rect.x, rect.y);
    add_round_rect(
        builder,
        RoundRect {
            x,
            y,
            width: rect.width * spec.scale,
            height: rect.height * spec.scale,
            radius: radius * spec.scale,
        },
    );
}

pub(super) fn push_rect(builder: &mut PathBuilder, spec: SubBarIconSpec, rect: IconRect) {
    let (x, y) = spec.point(rect.x, rect.y);
    if let Some(rect) =
        tiny_skia::Rect::from_xywh(x, y, rect.width * spec.scale, rect.height * spec.scale)
    {
        builder.push_rect(rect);
    }
}

pub(super) fn push_glyph_a(
    builder: &mut PathBuilder,
    spec: SubBarIconSpec,
    height: f32,
    width: f32,
) {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let (left, bottom) = spec.point(-half_width, half_height);
    let (top_x, top) = spec.point(0.0, -half_height);
    let (right, _) = spec.point(half_width, half_height);
    let (bar_left, bar_y) = spec.point(-half_width * 0.66, half_height * 0.36);
    let (bar_right, _) = spec.point(half_width * 0.66, half_height * 0.36);
    builder.move_to(left, bottom);
    builder.line_to(top_x, top);
    builder.line_to(right, bottom);
    builder.move_to(bar_left, bar_y);
    builder.line_to(bar_right, bar_y);
}

pub(super) fn with_alpha(color: Color, alpha: u8) -> Color {
    Color::from_rgba8(
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8,
        alpha,
    )
}
