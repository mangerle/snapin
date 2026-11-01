//! 像素网格马赛克遮挡图元实现。
//!
//! 对指定矩形区域内的像素按照步长进行降采样网格填充，达到隐私遮蔽效果。

use super::{rect_from_tuple, resize_rect, tuple_from_rect, valid_rect, SizeParam};
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

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

        let width = pixmap.width() as usize;
        let height = pixmap.height() as usize;
        if width == 0 || height == 0 {
            return;
        }

        // 先换算到整数像素边界；右/下边界向上取整，确保半像素覆盖也被遮住。
        let left = (rx.max(0.0).floor() as usize).min(width);
        let top = (ry.max(0.0).floor() as usize).min(height);
        let right = ((rx + rw).min(width as f32).ceil() as usize).min(width);
        let bottom = ((ry + rh).min(height as f32).ceil() as usize).min(height);
        if left >= right || top >= bottom {
            return;
        }

        let cell = self.cell_size.max(4) as usize;
        let stride = width * 4;
        let data = pixmap.data_mut();

        let mut block_top = top;
        while block_top < bottom {
            let block_bottom = (block_top + cell).min(bottom);
            let mut block_left = left;
            while block_left < right {
                let block_right = (block_left + cell).min(right);

                // 取块中心像素作为整块颜色，直接按行写入，绕过 tiny-skia 的
                // 逐格路径构造与抗锯齿插值，大区域下可减少数量级的光栅化开销。
                let sample_x = block_left + (block_right - block_left) / 2;
                let sample_y = block_top + (block_bottom - block_top) / 2;
                let sample_index = sample_y * width * 4 + sample_x * 4;
                let pixel = [
                    data[sample_index],
                    data[sample_index + 1],
                    data[sample_index + 2],
                    data[sample_index + 3],
                ];

                for row in block_top..block_bottom {
                    let start = row * stride + block_left * 4;
                    let end = row * stride + block_right * 4;
                    for target in data[start..end].as_chunks_mut::<4>().0 {
                        target.copy_from_slice(&pixel);
                    }
                }
                block_left = block_right;
            }
            block_top = block_bottom;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    /// 马赛克只改目标矩形内部，块内颜色一致且外部像素保持不变。
    #[test]
    fn test_mosaic_writes_uniform_blocks_inside_rect() {
        let mut pixmap = Pixmap::new(20, 20).expect("创建测试位图失败");
        pixmap.fill(Color::from_rgba8(10, 20, 30, 255));
        // 给首个 4x4 块中心写入不同颜色，便于验证区域确实被重写。
        let sample_index = (4 * 20 + 4) * 4;
        pixmap.data_mut()[sample_index..sample_index + 4].copy_from_slice(&[200, 50, 40, 255]);
        let before = pixmap.data().to_vec();

        let action = MosaicAction {
            rect: (2.0, 2.0, 16.0, 16.0),
            cell_size: 4,
        };
        action.draw(&mut pixmap.as_mut());

        let data = pixmap.data();
        let pixel = |x: usize, y: usize| {
            let idx = (y * 20 + x) * 4;
            [data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]
        };
        let sample = pixel(2, 2);
        assert_eq!(sample, [200, 50, 40, 255]);
        assert_eq!(pixel(5, 5), sample, "同一 4x4 块内颜色必须一致");
        assert_eq!(pixel(0, 0), [10, 20, 30, 255], "区域外像素不得被修改");
        assert_ne!(data, before.as_slice(), "区域内像素应发生变化");
    }
}
