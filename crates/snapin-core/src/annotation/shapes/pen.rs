//! 平滑自由画笔标注图元实现。

use super::{map_point, points_bounds};
use crate::annotation::action::{AnnotationAction, ShapeRect};
use serde::{Deserialize, Serialize};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 普通涂鸦画笔动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PenAction {
    pub points: Vec<(f32, f32)>,
    pub color: (u8, u8, u8, u8),
    pub stroke_width: f32,
}

impl PenAction {
    /// 直接基于点集切片在 Pixmap 上绘制，避免在预览和高频路径中分配临时 Vec
    pub fn draw_points_offset(
        points: &[(f32, f32)],
        color: (u8, u8, u8, u8),
        stroke_width: f32,
        pixmap: &mut PixmapMut,
        offset_x: f32,
        offset_y: f32,
    ) {
        if points.len() < 2 {
            return;
        }

        let mut paint = Paint::default();
        paint.set_color_rgba8(color.0, color.1, color.2, color.3);

        let stroke = Stroke {
            width: stroke_width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };

        let mut pb = PathBuilder::new();
        pb.move_to(points[0].0 + offset_x, points[0].1 + offset_y);
        for pt in &points[1..] {
            pb.line_to(pt.0 + offset_x, pt.1 + offset_y);
        }

        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

impl AnnotationAction for PenAction {
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        Self::draw_points_offset(
            &self.points,
            self.color,
            self.stroke_width,
            pixmap,
            offset_x,
            offset_y,
        );
    }

    fn adjust_size(&mut self, stroke_w: f32, _font_sz: f32) {
        self.stroke_width = stroke_w;
    }

    fn bounds(&self) -> Option<ShapeRect> {
        points_bounds(&self.points)
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        for point in &mut self.points {
            point.0 += dx;
            point.1 += dy;
        }
    }

    fn resize_from_bounds(&mut self, from: ShapeRect, to: ShapeRect) {
        for point in &mut self.points {
            *point = map_point(from, to, *point);
        }
    }
}
