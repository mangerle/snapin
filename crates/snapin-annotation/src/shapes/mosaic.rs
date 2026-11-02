//! 像素网格马赛克遮挡图元实现。
//!
//! 对指定矩形区域内的像素按照步长进行降采样网格填充，达到隐私遮蔽效果。

use super::{map_point, resize_rect, valid_rect, SizeParam};
use crate::geometry::{Point, Rect as GeometryRect};
use serde::{Deserialize, Serialize};
use tiny_skia::{
    LineCap, LineJoin, Mask, MaskType, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Rect,
    Stroke, Transform,
};

/// 马赛克遮罩动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MosaicAction {
    pub rect: GeometryRect,
    pub cell_size: u32,
    /// 画笔模式轨迹；为空时按矩形区域处理。
    #[serde(default)]
    pub points: Vec<Point>,
    /// 画笔模式直径。
    #[serde(default)]
    pub brush_width: f32,
}

impl MosaicAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        if self.points.is_empty() {
            self.draw_rect_offset(pixmap, offset_x, offset_y);
            return;
        }
        self.draw_brush_offset(pixmap, offset_x, offset_y);
    }

    fn draw_rect_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (mut rx, ry, rw, rh) = (self.rect.x, self.rect.y, self.rect.width, self.rect.height);
        rx += offset_x;
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

        apply_mosaic_rect(
            pixmap.data_mut(),
            width,
            MosaicBounds {
                left,
                top,
                right,
                bottom,
            },
            self.cell_size.max(4) as usize,
        );
    }

    fn draw_brush_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (rx, ry, rw, rh) = (self.rect.x, self.rect.y, self.rect.width, self.rect.height);
        let canvas_width = pixmap.width() as f32;
        let canvas_height = pixmap.height() as f32;
        let left = (rx + offset_x).max(0.0).floor() as usize;
        let top = (ry + offset_y).max(0.0).floor() as usize;
        let right =
            ((rx + rw + offset_x).min(canvas_width).ceil() as usize).min(pixmap.width() as usize);
        let bottom =
            ((ry + rh + offset_y).min(canvas_height).ceil() as usize).min(pixmap.height() as usize);
        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width == 0 || height == 0 {
            return;
        }

        let Some(mut effect_pixmap) = Pixmap::new(width as u32, height as u32) else {
            return;
        };
        let source_stride = pixmap.width() as usize * 4;
        let target_stride = width * 4;
        let source_data = pixmap.data_mut();
        let target_data = effect_pixmap.data_mut();
        for row in 0..height {
            let source_start = (top + row) * source_stride + left * 4;
            let target_start = row * target_stride;
            target_data[target_start..target_start + target_stride]
                .copy_from_slice(&source_data[source_start..source_start + target_stride]);
        }
        pixelate_region(target_data, width, height, self.cell_size.max(4) as usize);

        let Some(mask) = self.build_brush_mask(pixmap.width(), pixmap.height(), offset_x, offset_y)
        else {
            return;
        };
        pixmap.draw_pixmap(
            left as i32,
            top as i32,
            effect_pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            Some(&mask),
        );
    }

    fn build_brush_mask(
        &self,
        width: u32,
        height: u32,
        offset_x: f32,
        offset_y: f32,
    ) -> Option<Mask> {
        let mut mask_pixmap = Pixmap::new(width, height)?;
        let mut paint = Paint::default();
        paint.set_color(tiny_skia::Color::WHITE);
        if self.points.len() == 1 {
            let point = self.points[0];
            let radius = self.brush_width.max(1.0) / 2.0;
            if let Some(rect) = Rect::from_xywh(
                point.x + offset_x - radius,
                point.y + offset_y - radius,
                radius * 2.0,
                radius * 2.0,
            ) {
                mask_pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
        } else {
            let mut builder = PathBuilder::new();
            builder.move_to(self.points[0].x + offset_x, self.points[0].y + offset_y);
            for point in &self.points[1..] {
                builder.line_to(point.x + offset_x, point.y + offset_y);
            }
            let path = builder.finish()?;
            let stroke = Stroke {
                width: self.brush_width.max(1.0),
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };
            mask_pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
        Some(Mask::from_pixmap(mask_pixmap.as_ref(), MaskType::Alpha))
    }

    pub(crate) fn adjust_size(&mut self, param: SizeParam) {
        if let SizeParam::Stroke(width) = param {
            self.cell_size = (width * 2.0).clamp(4.0, 50.0) as u32;
            self.brush_width = width.max(1.0);
        }
    }

    pub(crate) fn bounds(&self) -> Option<GeometryRect> {
        valid_rect(self.rect)
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.rect.x += dx;
        self.rect.y += dy;
        for point in &mut self.points {
            point.x += dx;
            point.y += dy;
        }
    }

    pub(crate) fn resize_from_bounds(&mut self, from: GeometryRect, to: GeometryRect) {
        self.rect = resize_rect(from, to, self.rect);
        for point in &mut self.points {
            *point = map_point(from, to, *point);
        }
    }
}

fn pixelate_region(data: &mut [u8], width: usize, height: usize, cell: usize) {
    let cell = cell.max(4);
    let mut block_top = 0;
    while block_top < height {
        let block_bottom = (block_top + cell).min(height);
        let mut block_left = 0;
        while block_left < width {
            let block_right = (block_left + cell).min(width);
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
                let start = row * width * 4 + block_left * 4;
                let end = row * width * 4 + block_right * 4;
                for target in data[start..end].as_chunks_mut::<4>().0 {
                    target.copy_from_slice(&pixel);
                }
            }
            block_left = block_right;
        }
        block_top = block_bottom;
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl MosaicAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}

/// 整数像素马赛克区域。
#[derive(Clone, Copy)]
struct MosaicBounds {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

/// 将矩形区域内每个块替换为块中心颜色。
fn apply_mosaic_rect(data: &mut [u8], width: usize, bounds: MosaicBounds, cell: usize) {
    let stride = width * 4;
    let mut block_top = bounds.top;
    while block_top < bounds.bottom {
        let block_bottom = (block_top + cell).min(bounds.bottom);
        let mut block_left = bounds.left;
        while block_left < bounds.right {
            let block_right = (block_left + cell).min(bounds.right);
            let sample_x = block_left + (block_right - block_left) / 2;
            let sample_y = block_top + (block_bottom - block_top) / 2;
            let sample_index = sample_y * width * 4 + sample_x * 4;
            let pixel = [
                data[sample_index],
                data[sample_index + 1],
                data[sample_index + 2],
                data[sample_index + 3],
            ];
            fill_mosaic_block(
                data,
                stride,
                block_top,
                block_bottom,
                block_left,
                block_right,
                pixel,
            );
            block_left = block_right;
        }
        block_top = block_bottom;
    }
}

/// 用同一颜色填充一个马赛克块，按行直接写像素。
fn fill_mosaic_block(
    data: &mut [u8],
    stride: usize,
    top: usize,
    bottom: usize,
    left: usize,
    right: usize,
    pixel: [u8; 4],
) {
    for row in top..bottom {
        let start = row * stride + left * 4;
        let end = row * stride + right * 4;
        for target in data[start..end].as_chunks_mut::<4>().0 {
            target.copy_from_slice(&pixel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Pixmap};

    /// 画笔模式只处理轨迹覆盖区域，轨迹外像素必须保持不变。
    #[test]
    fn test_mosaic_brush_only_changes_stroke_area() {
        let mut pixmap = Pixmap::new(32, 24).expect("创建测试位图失败");
        pixmap.fill(Color::from_rgba8(10, 20, 30, 255));
        for y in 0..24 {
            for x in 0..32 {
                let index = (y * 32 + x) * 4;
                pixmap.data_mut()[index..index + 4].copy_from_slice(&[
                    (x * 7) as u8,
                    (y * 9) as u8,
                    180,
                    255,
                ]);
            }
        }
        let before = pixmap.data().to_vec();
        let action = MosaicAction {
            rect: GeometryRect::new(2.0, 8.0, 28.0, 8.0),
            cell_size: 6,
            points: vec![Point::new(4.0, 12.0), Point::new(28.0, 12.0)],
            brush_width: 6.0,
        };
        action.draw(&mut pixmap.as_mut());
        let center = (12 * 32 + 12) * 4;
        let outside = (2 * 32 + 2) * 4;
        assert_ne!(
            &pixmap.data()[center..center + 4],
            &before[center..center + 4]
        );
        assert_eq!(
            &pixmap.data()[outside..outside + 4],
            &before[outside..outside + 4]
        );
    }

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
            rect: GeometryRect::new(2.0, 2.0, 16.0, 16.0),
            cell_size: 4,
            points: Vec::new(),
            brush_width: 0.0,
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
