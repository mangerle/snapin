//! 平滑自由画笔标注图元实现。

use super::{map_point, points_bounds, SizeParam};
use crate::geometry::{Point, Rect};
use crate::style::{ColorRgba, LineStyle, StrokePathSpec};
use serde::{Deserialize, Serialize};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 高频画笔预览的点集、颜色、线宽、线型与偏移。
///
/// 预览阶段直接借用点集切片，避免每个鼠标移动事件都复制 `Vec<Point>`。
#[derive(Clone, Copy)]
pub struct PenStrokeSpec<'a> {
    pub points: &'a [Point],
    pub color: ColorRgba,
    pub stroke_width: f32,
    pub line_style: LineStyle,
    pub offset: Point,
}

/// 普通涂鸦画笔动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PenAction {
    pub points: Vec<Point>,
    pub color: ColorRgba,
    pub stroke_width: f32,
    /// 画笔轨迹线型；历史数据缺失时按实线恢复。
    #[serde(default)]
    pub line_style: LineStyle,
}

impl PenAction {
    /// 直接基于点集切片在 Pixmap 上绘制，避免在预览和高频路径中分配临时 Vec
    pub fn draw_points_offset(spec: PenStrokeSpec<'_>, pixmap: &mut PixmapMut) {
        if spec.points.len() < 2 {
            return;
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(spec.color.0, spec.color.1, spec.color.2, spec.color.3);

        let mut pb = PathBuilder::new();
        pb.move_to(
            spec.points[0].x + spec.offset.x,
            spec.points[0].y + spec.offset.y,
        );
        for point in &spec.points[1..] {
            pb.line_to(point.x + spec.offset.x, point.y + spec.offset.y);
        }

        if let Some(path) = pb.finish() {
            if spec.line_style == LineStyle::Solid {
                let stroke = Stroke {
                    width: spec.stroke_width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            } else {
                spec.line_style.stroke_path(
                    pixmap,
                    &paint,
                    StrokePathSpec::new(&path, spec.stroke_width),
                );
            }
        }
    }
}

impl PenAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        Self::draw_points_offset(
            PenStrokeSpec {
                points: &self.points,
                color: self.color,
                stroke_width: self.stroke_width,
                line_style: self.line_style,
                offset: Point::new(offset_x, offset_y),
            },
            pixmap,
        );
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Stroke(width) = param {
            self.stroke_width = width;
        }
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        points_bounds(&self.points)
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        for point in &mut self.points {
            *point = point.translated(dx, dy);
        }
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        for point in &mut self.points {
            *point = map_point(from, to, *point);
        }
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl PenAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}
