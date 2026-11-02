//! 指引箭头标注图元绘制实现。
//!
//! 箭头提供三种并列形态：实线箭杆、虚线箭杆、锥形填充箭杆（尾细头粗）。
//! 三者在同一份几何基础上绘制，只在箭杆渲染方式上分叉。

use super::{map_point, SizeParam};
use crate::geometry::{Point, Rect};
use crate::style::{ArrowHeadStyle, ArrowStyle, ColorRgba, LineStyle};
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// 指引箭头绘制动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowAction {
    pub start: Point,
    pub end: Point,
    pub color: ColorRgba,
    pub stroke_width: f32,
    /// 箭头形态：实线 / 虚线 / 锥形
    #[serde(default)]
    pub style: ArrowStyle,
    /// 线条样式
    #[serde(default)]
    pub line_style: LineStyle,
    /// 起点端点样式
    #[serde(default)]
    pub start_head: ArrowHeadStyle,
    /// 终点端点样式
    #[serde(default = "default_end_head")]
    pub end_head: ArrowHeadStyle,
}

fn default_end_head() -> ArrowHeadStyle {
    ArrowHeadStyle::Triangle
}

/// 箭头共享几何：单位方向、法向、头部基线与翼展半宽
struct ArrowGeometry {
    start: Point,
    end: Point,
    normal: (f32, f32),
    base: Point,
    head_half_w: f32,
}

impl ArrowAction {
    /// 计算箭头几何；过短的拖拽返回 `None`，避免头部倒退吞噬箭身
    fn geometry(&self) -> Option<ArrowGeometry> {
        let (x1, y1) = (self.start.x, self.start.y);
        let (x2, y2) = (self.end.x, self.end.y);
        let dx = x2 - x1;
        let dy = y2 - y1;
        let length = (dx * dx + dy * dy).sqrt();
        if length < 2.0 {
            return None;
        }

        let ux = dx / length;
        let uy = dy / length;

        let half_stroke = (self.stroke_width / 2.0).max(1.0);
        let mut head_len = (self.stroke_width * 3.5).clamp(16.0, 80.0);
        let mut head_half_w = (self.stroke_width * 2.2).max(half_stroke + 4.0);

        // 短距离箭头等比平滑缩放，防止头部倒退吞噬箭身
        if length < head_len * 1.25 {
            let scale = (length / (head_len * 1.25)).max(0.2);
            head_len *= scale;
            head_half_w *= scale;
        }

        Some(ArrowGeometry {
            start: self.start,
            end: self.end,
            normal: (-uy, ux),
            base: Point::new(x2 - ux * head_len, y2 - uy * head_len),
            head_half_w,
        })
    }

    fn draw_head_style(
        &self,
        pixmap: &mut PixmapMut,
        tip: Point,
        direction: (f32, f32),
        paint: &Paint,
        style: ArrowHeadStyle,
    ) {
        let (ux, uy) = direction;
        let (vx, vy) = (-uy, ux);
        let head_len = (self.stroke_width * 3.5).clamp(8.0, 48.0);
        let half_w = (self.stroke_width * 2.2).max(4.0);
        match style {
            ArrowHeadStyle::None => {}
            ArrowHeadStyle::Triangle | ArrowHeadStyle::Outline => {
                let base = Point::new(tip.x - ux * head_len, tip.y - uy * head_len);
                let mut pb = PathBuilder::new();
                pb.move_to(tip.x, tip.y);
                pb.line_to(base.x + vx * half_w, base.y + vy * half_w);
                pb.line_to(base.x - vx * half_w, base.y - vy * half_w);
                pb.close();
                if let Some(path) = pb.finish() {
                    if style == ArrowHeadStyle::Triangle {
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
            }
            ArrowHeadStyle::Bar => {
                let mut pb = PathBuilder::new();
                pb.move_to(tip.x + vx * half_w, tip.y + vy * half_w);
                pb.line_to(tip.x - vx * half_w, tip.y - vy * half_w);
                if let Some(path) = pb.finish() {
                    let stroke = Stroke {
                        width: self.stroke_width.max(1.0),
                        ..Default::default()
                    };
                    pixmap.stroke_path(&path, paint, &stroke, Transform::identity(), None);
                }
            }
            ArrowHeadStyle::Circle => {
                let mut pb = PathBuilder::new();
                pb.push_circle(tip.x, tip.y, half_w * 0.65);
                if let Some(path) = pb.finish() {
                    pixmap.fill_path(
                        &path,
                        paint,
                        tiny_skia::FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    }

    /// 锥形箭头：箭杆与头部合并为一体化填充多边形
    fn draw_tapered(
        &self,
        pixmap: &mut PixmapMut,
        geometry: &ArrowGeometry,
        paint: &Paint,
        line_style: LineStyle,
    ) {
        let (x1, y1) = (geometry.start.x, geometry.start.y);
        let (vx, vy) = geometry.normal;
        let (bx, by) = (geometry.base.x, geometry.base.y);
        let (x2, y2) = (geometry.end.x, geometry.end.y);
        let half_stroke = (self.stroke_width / 2.0).max(1.0);

        // 尾部收窄、向箭头方向线性加粗，形成毛笔式指向感
        let tail_half_w = (half_stroke * 0.32).max(0.8);

        let mut pb = PathBuilder::new();
        pb.move_to(x1 + vx * tail_half_w, y1 + vy * tail_half_w);
        pb.line_to(bx + vx * half_stroke, by + vy * half_stroke);
        pb.line_to(
            bx + vx * geometry.head_half_w,
            by + vy * geometry.head_half_w,
        );
        pb.line_to(x2, y2);
        pb.line_to(
            bx - vx * geometry.head_half_w,
            by - vy * geometry.head_half_w,
        );
        pb.line_to(bx - vx * half_stroke, by - vy * half_stroke);
        pb.line_to(x1 - vx * tail_half_w, y1 - vy * tail_half_w);
        pb.close();

        if let Some(path) = pb.finish() {
            if line_style == LineStyle::Solid {
                pixmap.fill_path(
                    &path,
                    paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            } else {
                line_style.stroke_path(pixmap, &path, paint, self.stroke_width.max(1.0));
            }
        }
    }

    /// 实线/虚线箭头：描边箭杆 + 独立实心头部
    fn draw_stroked(
        &self,
        pixmap: &mut PixmapMut,
        geometry: &ArrowGeometry,
        paint: &Paint,
        line_style: LineStyle,
    ) {
        let start = geometry.start;
        let base = geometry.base;

        let mut pb = PathBuilder::new();
        pb.move_to(start.x, start.y);
        pb.line_to(base.x, base.y);
        if let Some(path) = pb.finish() {
            line_style.stroke_path(pixmap, &path, paint, self.stroke_width);
        }
    }
}

impl ArrowAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let Some(mut geometry) = self.geometry() else {
            return;
        };
        geometry.start = geometry.start.translated(offset_x, offset_y);
        geometry.end = geometry.end.translated(offset_x, offset_y);
        geometry.base = geometry.base.translated(offset_x, offset_y);

        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

        match self.style.shaft_line_style() {
            Some(_) => self.draw_stroked(pixmap, &geometry, &paint, self.line_style),
            None => self.draw_tapered(pixmap, &geometry, &paint, self.line_style),
        }

        let dx = geometry.end.x - geometry.start.x;
        let dy = geometry.end.y - geometry.start.y;
        let length = (dx * dx + dy * dy).sqrt().max(1.0);
        let direction = (dx / length, dy / length);
        self.draw_head_style(
            pixmap,
            geometry.start,
            (-direction.0, -direction.1),
            &paint,
            self.start_head,
        );
        self.draw_head_style(pixmap, geometry.end, direction, &paint, self.end_head);
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Stroke(width) = param {
            self.stroke_width = width;
        }
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        let min_x = self.start.x.min(self.end.x);
        let min_y = self.start.y.min(self.end.y);
        let width = (self.end.x - self.start.x).abs();
        let height = (self.end.y - self.start.y).abs();
        if width < f32::EPSILON && height < f32::EPSILON {
            return None;
        }
        Some(Rect {
            x: min_x,
            y: min_y,
            width: width.max(1.0),
            height: height.max(1.0),
        })
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.start = self.start.translated(dx, dy);
        self.end = self.end.translated(dx, dy);
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.start = map_point(from, to, self.start);
        self.end = map_point(from, to, self.end);
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl ArrowAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    fn draw(style: ArrowStyle, stroke_width: f32) -> Pixmap {
        let mut pixmap = Pixmap::new(240, 120).expect("分配 Pixmap 成功");
        ArrowAction {
            start: Point::new(20.0, 60.0),
            end: Point::new(220.0, 60.0),
            color: (255, 0, 0, 255).into(),
            stroke_width,
            style,
            line_style: style.shaft_line_style().unwrap_or(LineStyle::Solid),
            start_head: ArrowHeadStyle::None,
            end_head: ArrowHeadStyle::Triangle,
        }
        .draw(&mut pixmap.as_mut());
        pixmap
    }

    /// 锥形箭杆：同一支箭头在尾部应明显窄于箭身中段（尾细头粗）
    #[test]
    fn test_arrow_shaft_tapers_toward_tail() {
        let pixmap = draw(ArrowStyle::Tapered, 12.0);
        let column_height = |x: u32| {
            (0..120)
                .filter(|y| pixmap.pixel(x, *y).map(|p| p.alpha()) == Some(255))
                .count()
        };
        let tail = column_height(28);
        let middle = column_height(120);
        assert!(tail > 0, "箭尾应被着色，实际列高 {tail}");
        assert!(
            tail < middle,
            "尾部应窄于箭身中段，实际 尾={tail} 中={middle}"
        );
    }

    /// 实线箭杆连续，虚线箭杆必须出现间隔；两种样式的头部都应实心
    #[test]
    fn test_arrow_solid_and_dashed_shafts() {
        let solid = draw(ArrowStyle::Solid, 6.0);
        let dashed = draw(ArrowStyle::Dashed, 6.0);

        let painted = |pixmap: &Pixmap| {
            (40..140)
                .filter(|x| pixmap.pixel(*x, 60).map(|p| p.alpha()).unwrap_or(0) > 200)
                .count()
        };
        let solid_count = painted(&solid);
        let dashed_count = painted(&dashed);

        assert_eq!(solid_count, 100, "实线箭杆应连续着色");
        assert!(
            dashed_count < 80 && dashed_count > 20,
            "虚线箭杆应出现明显间隔，实际着色 {dashed_count}/100"
        );

        for pixmap in [&solid, &dashed] {
            let tip = pixmap.pixel(214, 60).map(|p| p.alpha()).unwrap_or(0);
            assert!(tip > 0, "箭头头部应被填充");
        }
    }

    /// 描边箭杆的粗细变化不影响头部几何有效性
    #[test]
    fn test_arrow_drawing_and_size_adjustment() {
        let mut pixmap = Pixmap::new(200, 200).expect("分配 Pixmap 成功");
        let mut arrow = ArrowAction {
            start: Point::new(20.0, 20.0),
            end: Point::new(180.0, 180.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 8.0,
            style: ArrowStyle::Tapered,
            line_style: LineStyle::Solid,
            start_head: ArrowHeadStyle::None,
            end_head: ArrowHeadStyle::Triangle,
        };

        arrow.draw(&mut pixmap.as_mut());
        let tip_idx = ((175 * 200 + 175) * 4) as usize;
        assert_eq!(pixmap.data()[tip_idx], 255);
        assert_eq!(pixmap.data()[tip_idx + 3], 255);

        arrow.adjust_size(SizeParam::Stroke(24.0));
        assert_eq!(arrow.stroke_width, 24.0);
        arrow.draw(&mut pixmap.as_mut());
    }
}
