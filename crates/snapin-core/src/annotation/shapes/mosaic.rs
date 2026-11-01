//! 像素网格马赛克遮挡图元实现。
//!
//! 对指定矩形区域内的像素按照步长进行降采样网格填充，达到隐私遮蔽效果。

use super::{rect_from_tuple, resize_rect, tuple_from_rect, valid_rect, SizeParam};
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PixmapMut, Rect as SkiaRect, Transform};

/// 马赛克遮罩动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MosaicAction {
    pub rect: (f32, f32, f32, f32),
    pub cell_size: u32,
}

impl MosaicAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (rx, ry, rw, rh) = self.rect;
        let rx = rx + offset_x;
        let ry = ry + offset_y;
        let cell = self.cell_size.max(4) as f32;

        let start_x = rx.max(0.0);
        let start_y = ry.max(0.0);
        let end_x = (rx + rw).min(pixmap.width() as f32);
        let end_y = (ry + rh).min(pixmap.height() as f32);

        let width = pixmap.width() as usize;
        let height = pixmap.height() as usize;

        let mut y = start_y;
        while y < end_y {
            let mut x = start_x;
            while x < end_x {
                let block_w = cell.min(end_x - x);
                let block_h = cell.min(end_y - y);

                let sample_x = (x + block_w / 2.0) as usize;
                let sample_y = (y + block_h / 2.0) as usize;

                if sample_x < width && sample_y < height {
                    let idx = (sample_y * width + sample_x) * 4;
                    let (r, g, b, a) = {
                        let data = pixmap.data_mut();
                        (data[idx], data[idx + 1], data[idx + 2], data[idx + 3])
                    };

                    if let Some(r_rect) = SkiaRect::from_xywh(x, y, block_w, block_h) {
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(r, g, b, a);
                        pixmap.fill_rect(r_rect, &paint, Transform::identity(), None);
                    }
                }
                x += cell;
            }
            y += cell;
        }
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Stroke(width) = param {
            self.cell_size = (width * 2.0).clamp(4.0, 50.0) as u32;
        }
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

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl MosaicAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}
