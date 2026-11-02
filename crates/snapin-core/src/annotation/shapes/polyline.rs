//! 直线与折线标注图元。

use super::{map_point, points_bounds, SizeParam};
use crate::annotation::style::{ArrowHeadStyle, ColorRgba, LineStyle};
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 直线或折线标注。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolylineAction {
    pub points: Vec<(f32, f32)>,
    pub color: ColorRgba,
    pub stroke_width: f32,
    #[serde(default)]
    pub line_style: LineStyle,
    #[serde(default)]
    pub start_head: ArrowHeadStyle,
    #[serde(default)]
    pub end_head: ArrowHeadStyle,
}

impl PolylineAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        if self.points.len() < 2 {
            return;
        }
        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);
        let mut pb = PathBuilder::new();
        pb.move_to(self.points[0].0 + offset_x, self.points[0].1 + offset_y);
        for point in &self.points[1..] {
            pb.line_to(point.0 + offset_x, point.1 + offset_y);
        }
        let Some(path) = pb.finish() else {
            return;
        };
        self.line_style
            .stroke_path(pixmap, &path, &paint, self.stroke_width);
        self.draw_heads(pixmap, offset_x, offset_y, &paint);
    }

    fn draw_heads(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32, paint: &Paint) {
        let Some(first) = self.points.first().copied() else {
            return;
        };
        let Some(last) = self.points.last().copied() else {
            return;
        };
        let first_next = self.points.get(1).copied().unwrap_or(last);
        self.draw_head(
            pixmap,
            (first.0 + offset_x, first.1 + offset_y),
            direction(first, first_next),
            self.start_head,
            paint,
        );
        let last_prev = self
            .points
            .get(self.points.len().saturating_sub(2))
            .copied()
            .unwrap_or(first);
        self.draw_head(
            pixmap,
            (last.0 + offset_x, last.1 + offset_y),
            direction(last_prev, last),
            self.end_head,
            paint,
        );
    }

    fn draw_head(
        &self,
        pixmap: &mut PixmapMut,
        tip: (f32, f32),
        direction: (f32, f32),
        style: ArrowHeadStyle,
        paint: &Paint,
    ) {
        if style == ArrowHeadStyle::None {
            return;
        }
        let (ux, uy) = direction;
        let (vx, vy) = (-uy, ux);
        let length = (self.stroke_width * 3.5).clamp(8.0, 48.0);
        let half = (self.stroke_width * 2.2).max(4.0);
        let base = (tip.0 - ux * length, tip.1 - uy * length);
        let mut pb = PathBuilder::new();
        match style {
            ArrowHeadStyle::Triangle | ArrowHeadStyle::Outline => {
                pb.move_to(tip.0, tip.1);
                pb.line_to(base.0 + vx * half, base.1 + vy * half);
                pb.line_to(base.0 - vx * half, base.1 - vy * half);
                pb.close();
            }
            ArrowHeadStyle::Bar => {
                pb.move_to(tip.0 + vx * half, tip.1 + vy * half);
                pb.line_to(tip.0 - vx * half, tip.1 - vy * half);
            }
            ArrowHeadStyle::Circle => {
                pb.push_circle(tip.0, tip.1, half * 0.65);
            }
            ArrowHeadStyle::None => return,
        }
        let Some(path) = pb.finish() else {
            return;
        };
        if matches!(style, ArrowHeadStyle::Triangle | ArrowHeadStyle::Circle) {
            pixmap.fill_path(
                &path,
                paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        } else {
            let stroke = Stroke {
                width: self.stroke_width.max(1.0),
                ..Default::default()
            };
            pixmap.stroke_path(&path, paint, &stroke, Transform::identity(), None);
        }
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
            point.0 += dx;
            point.1 += dy;
        }
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        for point in &mut self.points {
            *point = map_point(from, to, *point);
        }
    }
}

fn direction(from: (f32, f32), to: (f32, f32)) -> (f32, f32) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let length = (dx * dx + dy * dy).sqrt().max(1.0);
    (dx / length, dy / length)
}

impl PolylineAction {
    pub fn draw(&self, pixmap: &mut PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    #[test]
    fn test_polyline_draws_all_segments() {
        let mut pixmap = Pixmap::new(100, 100).expect("创建位图失败");
        pixmap.fill(Color::WHITE);
        PolylineAction {
            points: vec![(10.0, 10.0), (50.0, 10.0), (50.0, 80.0)],
            color: (255, 0, 0, 255).into(),
            stroke_width: 4.0,
            line_style: LineStyle::Solid,
            start_head: ArrowHeadStyle::None,
            end_head: ArrowHeadStyle::Triangle,
        }
        .draw(&mut pixmap.as_mut());
        assert!(pixmap.pixel(30, 10).is_some_and(|pixel| pixel.alpha() > 0));
        assert!(pixmap.pixel(50, 50).is_some_and(|pixel| pixel.alpha() > 0));
    }
}
