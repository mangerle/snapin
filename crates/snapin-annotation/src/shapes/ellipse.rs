//! 椭圆与圆形标注图元绘制实现。

use super::{resize_rect, valid_rect, SizeParam};
use crate::geometry::Rect;
use crate::style::{ColorRgba, LineStyle};
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Rect as SkiaRect, Transform};

/// 椭圆与圆形绘制动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EllipseAction {
    pub rect: Rect,
    pub color: ColorRgba,
    pub stroke_width: f32,
    pub is_filled: bool,
    /// 描边线型，仅在非填充模式生效
    #[serde(default)]
    pub line_style: LineStyle,
}

impl EllipseAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (x, y, w, h) = (self.rect.x, self.rect.y, self.rect.width, self.rect.height);
        if let Some(r) = SkiaRect::from_xywh(x + offset_x, y + offset_y, w, h) {
            let mut paint = Paint::default();
            paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);

            let mut pb = PathBuilder::new();
            pb.push_oval(r);
            if let Some(path) = pb.finish() {
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
        }
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Stroke(width) = param {
            self.stroke_width = width;
        }
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        valid_rect(self.rect)
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.rect.x += dx;
        self.rect.y += dy;
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.rect = resize_rect(from, to, self.rect);
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl EllipseAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}
