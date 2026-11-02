//! 平滑自由画笔标注图元实现。

use super::{map_point, points_bounds, SizeParam};
use crate::geometry::{Point, Rect};
use crate::style::{ColorRgba, LineStyle};
use serde::{Deserialize, Serialize};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

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
    pub fn draw_points_offset(
        points: &[Point],
        color: (u8, u8, u8, u8),
        stroke_width: f32,
        line_style: LineStyle,
        pixmap: &mut PixmapMut,
        offset_x: f32,
        offset_y: f32,
    ) {
        if points.len() < 2 {
            return;
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(color.0, color.1, color.2, color.3);

        let mut pb = PathBuilder::new();
        pb.move_to(points[0].x + offset_x, points[0].y + offset_y);
        for pt in &points[1..] {
            pb.line_to(pt.x + offset_x, pt.y + offset_y);
        }

        if let Some(path) = pb.finish() {
            if line_style == LineStyle::Solid {
                let stroke = Stroke {
                    width: stroke_width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            } else {
                line_style.stroke_path(pixmap, &path, &paint, stroke_width);
            }
        }
    }
}

impl PenAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        Self::draw_points_offset(
            &self.points,
            self.color.to_tuple(),
            self.stroke_width,
            self.line_style,
            pixmap,
            offset_x,
            offset_y,
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
