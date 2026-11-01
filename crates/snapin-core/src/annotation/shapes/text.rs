//! 文本标注图元绘制实现。

use super::{estimate_text_width, map_point, SizeParam};
use crate::annotation::style::ColorRgba;
use crate::geometry::Rect;
use crate::render::with_thread_local_text_renderer;
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

/// 文本标注图元动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextAction {
    pub pos: (f32, f32),
    pub text: String,
    pub color: ColorRgba,
    pub font_size: f32,
}

impl TextAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        if self.text.is_empty() {
            return;
        }

        let (x, y) = (self.pos.0 + offset_x, self.pos.1 + offset_y);
        let font_size = self.font_size.max(8.0);

        with_thread_local_text_renderer(|renderer| {
            renderer.draw_text_rgba(pixmap, &self.text, x, y, font_size, self.color.to_tuple());
        });
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Font(size) = param {
            self.font_size = size;
        }
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        if self.text.is_empty() {
            return None;
        }
        let width = estimate_text_width(&self.text, self.font_size).max(1.0);
        let height = (self.font_size * 1.25).max(1.0);
        Some(Rect {
            x: self.pos.0,
            y: self.pos.1,
            width,
            height,
        })
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.pos.0 += dx;
        self.pos.1 += dy;
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.pos = map_point(from, to, self.pos);
        let sy = if from.height.abs() > f32::EPSILON {
            to.height / from.height
        } else {
            1.0
        };
        self.font_size = (self.font_size * sy).clamp(8.0, 240.0);
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl TextAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}
