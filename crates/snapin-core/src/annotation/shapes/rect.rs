//! 矩形标注图元绘制实现。

use super::{rect_from_tuple, resize_rect, tuple_from_rect, valid_rect, SizeParam};
use crate::annotation::style::{ColorRgba, LineStyle};
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, Path, PathBuilder, PixmapMut, Transform};

/// 矩形绘制动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RectAction {
    pub rect: (f32, f32, f32, f32),
    pub color: ColorRgba,
    pub stroke_width: f32,
    pub is_filled: bool,
    /// 圆角半径（像素），0 表示直角；运行时自动收敛到短边一半以内
    #[serde(default)]
    pub corner_radius: f32,
    /// 描边线型，仅在非填充模式生效
    #[serde(default)]
    pub line_style: LineStyle,
}

impl RectAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (x, y, w, h) = self.rect;
        if w <= 0.0 || h <= 0.0 {
            return;
        }

        let Some(path) = rounded_rect_path(x + offset_x, y + offset_y, w, h, self.corner_radius)
        else {
            return;
        };

        let mut paint = Paint::default();
        paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

        if self.is_filled {
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        self.line_style
            .stroke_path(pixmap, &path, &paint, self.stroke_width);
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Stroke(width) = param {
            self.stroke_width = width;
        }
    }

    pub(crate) fn adjust_corner_radius(&mut self, radius: f32) {
        self.corner_radius = radius.clamp(0.0, 90.0);
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        valid_rect(rect_from_tuple(self.rect))
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.rect.0 += dx;
        self.rect.1 += dy;
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.rect = tuple_from_rect(resize_rect(from, to, rect_from_tuple(self.rect)));
    }
}

/// 构造圆角矩形路径。
///
/// tiny-skia 0.11 未提供圆角矩形图元，这里用四段二次贝塞尔手工拼装；
/// 半径自动收敛到短边一半，半径过大时不会产生自交路径。
pub(crate) fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    let r = radius.clamp(0.0, (w.min(h) / 2.0).max(0.0));
    let mut pb = PathBuilder::new();

    if r <= 0.5 {
        pb.push_rect(tiny_skia::Rect::from_xywh(x, y, w, h)?);
        return pb.finish();
    }

    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl RectAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    fn draw_rect(style: LineStyle, corner_radius: f32) -> Pixmap {
        let mut pixmap = Pixmap::new(180, 140).expect("分配 Pixmap 成功");
        RectAction {
            rect: (20.0, 20.0, 120.0, 80.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 4.0,
            is_filled: false,
            corner_radius,
            line_style: style,
        }
        .draw(&mut pixmap.as_mut());
        pixmap
    }

    /// 实线应连续覆盖顶边，虚线必须在同一段上留下空隙
    #[test]
    fn test_rect_dashed_line_leaves_gaps() {
        let solid = draw_rect(LineStyle::Solid, 0.0);
        let dashed = draw_rect(LineStyle::Dashed, 0.0);

        let opaque = |pixmap: &Pixmap| {
            (30..130)
                .filter(|x| pixmap.pixel(*x, 20).map(|p| p.alpha()).unwrap_or(0) > 200)
                .count()
        };
        let solid_count = opaque(&solid);
        let dashed_count = opaque(&dashed);

        assert_eq!(solid_count, 100, "实线顶边应连续着色");
        assert!(
            dashed_count < 85 && dashed_count > 20,
            "虚线应出现明显间隔，实际着色 {dashed_count}/100"
        );
    }

    /// 圆角矩形应削掉直角顶点，直角矩形则应覆盖该点
    #[test]
    fn test_rect_corner_radius_trims_sharp_corner() {
        let sharp = draw_rect(LineStyle::Solid, 0.0);
        let rounded = draw_rect(LineStyle::Solid, 20.0);

        let alpha_at =
            |pixmap: &Pixmap, x: u32, y: u32| pixmap.pixel(x, y).map(|p| p.alpha()).unwrap_or(0);
        assert!(alpha_at(&sharp, 21, 21) > 0, "直角矩形的顶点应被描边覆盖");
        assert_eq!(
            alpha_at(&rounded, 21, 21),
            0,
            "圆角半径 20 时顶点应被削成空白"
        );
        // 圆角弧线中段仍应着色，确认不是整段描边丢失
        assert!(alpha_at(&rounded, 20, 40) > 0, "圆角左侧弧线应正常描边");
    }

    /// 参考工具栏提供 90 像素档位，矩形圆角不能再被旧的 30 像素上限截断。
    #[test]
    fn test_rect_corner_radius_supports_ninety_pixels() {
        let mut action = RectAction {
            rect: (0.0, 0.0, 200.0, 120.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        };
        action.adjust_corner_radius(90.0);
        assert_eq!(action.corner_radius, 90.0);
    }
}
