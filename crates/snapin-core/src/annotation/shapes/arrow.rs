//! 指引箭头标注图元绘制实现。
//!
//! 箭头提供三种并列形态：实线箭杆、虚线箭杆、锥形填充箭杆（尾细头粗）。
//! 三者在同一份几何基础上绘制，只在箭杆渲染方式上分叉。

use super::map_point;
use crate::annotation::action::{AnnotationAction, ShapeRect};
use crate::annotation::style::{ArrowStyle, LineStyle};
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Transform};

/// 指引箭头绘制动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowAction {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub color: (u8, u8, u8, u8),
    pub stroke_width: f32,
    /// 箭头形态：实线 / 虚线 / 锥形
    #[serde(default)]
    pub style: ArrowStyle,
}

/// 箭头共享几何：单位方向、法向、头部基线与翼展半宽
struct ArrowGeometry {
    start: (f32, f32),
    end: (f32, f32),
    normal: (f32, f32),
    base: (f32, f32),
    head_half_w: f32,
}

impl ArrowAction {
    /// 计算箭头几何；过短的拖拽返回 `None`，避免头部倒退吞噬箭身
    fn geometry(&self) -> Option<ArrowGeometry> {
        let (x1, y1) = self.start;
        let (x2, y2) = self.end;
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
            start: (x1, y1),
            end: (x2, y2),
            normal: (-uy, ux),
            base: (x2 - ux * head_len, y2 - uy * head_len),
            head_half_w,
        })
    }

    /// 绘制实心箭头头部
    fn draw_head(&self, pixmap: &mut PixmapMut, geometry: &ArrowGeometry, paint: &Paint) {
        let (vx, vy) = geometry.normal;
        let (bx, by) = geometry.base;
        let (x2, y2) = geometry.end;

        let mut pb = PathBuilder::new();
        pb.move_to(x2, y2);
        pb.line_to(
            bx + vx * geometry.head_half_w,
            by + vy * geometry.head_half_w,
        );
        pb.line_to(
            bx - vx * geometry.head_half_w,
            by - vy * geometry.head_half_w,
        );
        pb.close();
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

    /// 锥形箭头：箭杆与头部合并为一体化填充多边形
    fn draw_tapered(&self, pixmap: &mut PixmapMut, geometry: &ArrowGeometry, paint: &Paint) {
        let (x1, y1) = geometry.start;
        let (vx, vy) = geometry.normal;
        let (bx, by) = geometry.base;
        let (x2, y2) = geometry.end;
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
            pixmap.fill_path(
                &path,
                paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
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
        let (x1, y1) = geometry.start;
        let (bx, by) = geometry.base;

        let mut pb = PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(bx, by);
        if let Some(path) = pb.finish() {
            line_style.stroke_path(pixmap, &path, paint, self.stroke_width);
        }

        self.draw_head(pixmap, geometry, paint);
    }
}

impl AnnotationAction for ArrowAction {
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let Some(mut geometry) = self.geometry() else {
            return;
        };
        geometry.start = (geometry.start.0 + offset_x, geometry.start.1 + offset_y);
        geometry.end = (geometry.end.0 + offset_x, geometry.end.1 + offset_y);
        geometry.base = (geometry.base.0 + offset_x, geometry.base.1 + offset_y);

        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

        match self.style.shaft_line_style() {
            Some(line_style) => self.draw_stroked(pixmap, &geometry, &paint, line_style),
            None => self.draw_tapered(pixmap, &geometry, &paint),
        }
    }

    fn adjust_size(&mut self, stroke_w: f32, _font_sz: f32) {
        self.stroke_width = stroke_w;
    }

    fn bounds(&self) -> Option<ShapeRect> {
        let min_x = self.start.0.min(self.end.0);
        let min_y = self.start.1.min(self.end.1);
        let width = (self.end.0 - self.start.0).abs();
        let height = (self.end.1 - self.start.1).abs();
        if width < f32::EPSILON && height < f32::EPSILON {
            return None;
        }
        Some((min_x, min_y, width.max(1.0), height.max(1.0)))
    }

    fn translate(&mut self, dx: f32, dy: f32) {
        self.start.0 += dx;
        self.start.1 += dy;
        self.end.0 += dx;
        self.end.1 += dy;
    }

    fn resize_from_bounds(&mut self, from: ShapeRect, to: ShapeRect) {
        self.start = map_point(from, to, self.start);
        self.end = map_point(from, to, self.end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    fn draw(style: ArrowStyle, stroke_width: f32) -> Pixmap {
        let mut pixmap = Pixmap::new(240, 120).expect("分配 Pixmap 成功");
        ArrowAction {
            start: (20.0, 60.0),
            end: (220.0, 60.0),
            color: (255, 0, 0, 255),
            stroke_width,
            style,
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
            start: (20.0, 20.0),
            end: (180.0, 180.0),
            color: (255, 0, 0, 255),
            stroke_width: 8.0,
            style: ArrowStyle::Tapered,
        };

        arrow.draw(&mut pixmap.as_mut());
        let tip_idx = ((175 * 200 + 175) * 4) as usize;
        assert_eq!(pixmap.data()[tip_idx], 255);
        assert_eq!(pixmap.data()[tip_idx + 3], 255);

        arrow.adjust_size(24.0, 0.0);
        assert_eq!(arrow.stroke_width, 24.0);
        arrow.draw(&mut pixmap.as_mut());
    }
}
