//! 放大镜连接线与几何辅助。

use super::MagnifierShape;
use crate::geometry::{Point, Rect};
use tiny_skia::{PathBuilder, Rect as SkiaRect};

/// 放大镜连接线路径与端点。
pub(super) struct ConnectorGeometry {
    pub(super) path: tiny_skia::Path,
    pub(super) start: Point,
    pub(super) end: Point,
}

/// 放大镜连接线的端点、法向与端帽半长。
pub(super) struct ConnectorLine {
    pub(super) start: Point,
    pub(super) end: Point,
    pub(super) normal: (f32, f32),
    pub(super) half: f32,
}

impl ConnectorLine {
    pub(super) fn new(start: Point, end: Point, border_width: f32) -> Self {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = (dx * dx + dy * dy).sqrt().max(1.0);
        Self {
            start,
            end,
            normal: (-dy / length, dx / length),
            half: (border_width * 4.0).clamp(4.0, 16.0),
        }
    }
}

pub(super) fn push_shape_path(builder: &mut PathBuilder, shape: MagnifierShape, rect: SkiaRect) {
    match shape {
        MagnifierShape::Rectangle => builder.push_rect(rect),
        MagnifierShape::Ellipse => builder.push_oval(rect),
    }
}

pub(super) fn offset_rect(rect: Rect, offset_x: f32, offset_y: f32) -> Rect {
    Rect {
        x: rect.x + offset_x,
        y: rect.y + offset_y,
        ..rect
    }
}

pub(super) fn clamp_rect(rect: Rect, width: u32, height: u32) -> Option<Rect> {
    let left = rect.x.max(0.0).min(width as f32);
    let top = rect.y.max(0.0).min(height as f32);
    let right = (rect.x + rect.width).max(0.0).min(width as f32);
    let bottom = (rect.y + rect.height).max(0.0).min(height as f32);
    (right > left && bottom > top).then_some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

pub(super) fn union_rects(a: Rect, b: Rect) -> Option<Rect> {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    (right > left && bottom > top).then_some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

/// 计算矩形中心。
pub(super) fn rect_center(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

/// 按方向求矩形边界交点；方向退化为零时返回中心。
pub(super) fn boundary_point(
    center: Point,
    half_width: f32,
    half_height: f32,
    direction: (f32, f32),
) -> Point {
    let scale_x = if direction.0.abs() > f32::EPSILON {
        half_width / direction.0.abs()
    } else {
        f32::INFINITY
    };
    let scale_y = if direction.1.abs() > f32::EPSILON {
        half_height / direction.1.abs()
    } else {
        f32::INFINITY
    };
    let scale = scale_x.min(scale_y);
    if scale.is_finite() {
        Point::new(
            center.x + direction.0 * scale,
            center.y + direction.1 * scale,
        )
    } else {
        center
    }
}

/// 两个矩形是否存在面积重叠。
pub(super) fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
