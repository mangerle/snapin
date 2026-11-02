//! 直线与折线标注图元。

use super::{map_point, points_bounds, SizeParam};
use crate::geometry::{Point, Rect};
use crate::style::{ArrowHeadStyle, ColorRgba, LineStyle, StrokePathSpec};
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 直线或折线标注。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolylineAction {
    pub points: Vec<Point>,
    pub color: ColorRgba,
    pub stroke_width: f32,
    #[serde(default)]
    pub line_style: LineStyle,
    #[serde(default)]
    pub start_head: ArrowHeadStyle,
    #[serde(default)]
    pub end_head: ArrowHeadStyle,
}

/// 折线端点箭头绘制参数。
struct PolylineHeadDraw {
    tip: Point,
    direction: (f32, f32),
    style: ArrowHeadStyle,
}

impl PolylineAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        if self.points.len() < 2 {
            return;
        }
        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);
        let mut pb = PathBuilder::new();
        pb.move_to(self.points[0].x + offset_x, self.points[0].y + offset_y);
        for point in &self.points[1..] {
            pb.line_to(point.x + offset_x, point.y + offset_y);
        }
        let Some(path) = pb.finish() else {
            return;
        };
        self.line_style.stroke_path(
            pixmap,
            &paint,
            StrokePathSpec::new(&path, self.stroke_width),
        );
        self.draw_heads(pixmap, Point::new(offset_x, offset_y), &paint);
    }

    fn draw_heads(&self, pixmap: &mut PixmapMut, offset: Point, paint: &Paint) {
        let Some(first) = self.points.first().copied() else {
            return;
        };
        let Some(last) = self.points.last().copied() else {
            return;
        };
        let first_next = self.points.get(1).copied().unwrap_or(last);
        self.draw_head(
            pixmap,
            paint,
            PolylineHeadDraw {
                tip: first.translated(offset.x, offset.y),
                direction: direction(first, first_next),
                style: self.start_head,
            },
        );
        let last_prev = self
            .points
            .get(self.points.len().saturating_sub(2))
            .copied()
            .unwrap_or(first);
        self.draw_head(
            pixmap,
            paint,
            PolylineHeadDraw {
                tip: last.translated(offset.x, offset.y),
                direction: direction(last_prev, last),
                style: self.end_head,
            },
        );
    }

    fn draw_head(&self, pixmap: &mut PixmapMut, paint: &Paint, head: PolylineHeadDraw) {
        let PolylineHeadDraw {
            tip,
            direction,
            style,
        } = head;
        if style == ArrowHeadStyle::None {
            return;
        }
        let (ux, uy) = direction;
        let (vx, vy) = (-uy, ux);
        let length = (self.stroke_width * 3.5).clamp(8.0, 48.0);
        let half = (self.stroke_width * 2.2).max(4.0);
        let base = Point::new(tip.x - ux * length, tip.y - uy * length);
        let mut pb = PathBuilder::new();
        match style {
            ArrowHeadStyle::Triangle | ArrowHeadStyle::Outline => {
                pb.move_to(tip.x, tip.y);
                pb.line_to(base.x + vx * half, base.y + vy * half);
                pb.line_to(base.x - vx * half, base.y - vy * half);
                pb.close();
            }
            ArrowHeadStyle::Bar => {
                pb.move_to(tip.x + vx * half, tip.y + vy * half);
                pb.line_to(tip.x - vx * half, tip.y - vy * half);
            }
            ArrowHeadStyle::Circle => {
                pb.push_circle(tip.x, tip.y, half * 0.65);
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
            *point = point.translated(dx, dy);
        }
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        for point in &mut self.points {
            *point = map_point(from, to, *point);
        }
    }
}

fn direction(from: Point, to: Point) -> (f32, f32) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
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
            points: vec![
                Point::new(10.0, 10.0),
                Point::new(50.0, 10.0),
                Point::new(50.0, 80.0),
            ],
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
