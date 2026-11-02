//! 局部模糊遮挡图元。
//!
//! 对指定矩形区域执行两轮可分离盒式模糊，近似高斯模糊效果。模糊在撤销/重做时
//! 从底图重新计算，而不是保存像素快照，因此历史栈保持轻量且结果确定。

use super::{rect_from_tuple, resize_rect, tuple_from_rect, valid_rect, SizeParam};
use crate::geometry::Rect;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use tiny_skia::{
    LineCap, LineJoin, Mask, MaskType, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Stroke,
    Transform,
};

/// 模糊绘制复用缓冲，避免每次撤销/重做都分配四份区域大小的像素内存。
#[derive(Default)]
struct BlurScratch {
    region: Vec<u8>,
    blurred: Vec<u8>,
    current: Vec<u8>,
    scratch: Vec<u8>,
}

thread_local! {
    static BLUR_SCRATCH: RefCell<BlurScratch> = RefCell::new(BlurScratch::default());
}

/// 局部模糊动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlurAction {
    pub rect: (f32, f32, f32, f32),
    /// 模糊半径（像素），实际计算时夹取到 1..=50
    pub radius: u32,
    /// 画笔模式轨迹；为空时按矩形区域处理。
    #[serde(default)]
    pub points: Vec<(f32, f32)>,
    /// 画笔模式直径。
    #[serde(default)]
    pub brush_width: f32,
}

/// 已裁剪到画布范围的像素区域。
#[derive(Clone, Copy)]
struct PixelRegion {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

impl BlurAction {
    pub(crate) fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        if !self.points.is_empty() {
            self.draw_brush_offset(pixmap, offset_x, offset_y);
            return;
        }
        self.draw_rect_offset(pixmap, offset_x, offset_y);
    }

    /// 计算图元矩形在当前画布中的有效像素范围。
    fn pixel_region(
        &self,
        pixmap: &PixmapMut,
        offset_x: f32,
        offset_y: f32,
    ) -> Option<PixelRegion> {
        let (rx, ry, rw, rh) = self.rect;
        let width = pixmap.width() as f32;
        let height = pixmap.height() as f32;
        let left = (rx + offset_x).max(0.0).floor() as usize;
        let top = (ry + offset_y).max(0.0).floor() as usize;
        let right = (rx + rw + offset_x).min(width).ceil() as usize;
        let bottom = (ry + rh + offset_y).min(height).ceil() as usize;
        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        (width > 0 && height > 0).then_some(PixelRegion {
            left,
            top,
            width,
            height,
        })
    }

    /// 对矩形区域执行两轮可分离盒式模糊。
    fn draw_rect_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let Some(region) = self.pixel_region(pixmap, offset_x, offset_y) else {
            return;
        };
        let row_bytes = region.width * 4;
        BLUR_SCRATCH.with(|cell| {
            let mut scratch = cell.borrow_mut();
            let BlurScratch {
                region: pixels,
                blurred,
                current,
                scratch: work,
            } = &mut *scratch;
            pixels.clear();
            pixels.resize(region.height * row_bytes, 0);
            copy_rows_from_pixmap(pixmap, &region, pixels);
            blurred.clear();
            blurred.resize(pixels.len(), 0);
            box_blur_rgba_with_scratch(
                pixels,
                blurred,
                region.width,
                region.height,
                self.radius.clamp(1, 50) as usize,
                current,
                work,
            );
            copy_rows_to_pixmap(pixmap, &region, blurred);
        });
    }

    fn draw_brush_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let Some(region) = self.pixel_region(pixmap, offset_x, offset_y) else {
            return;
        };
        let Some(effect_pixmap) = self.build_blurred_region(pixmap, &region) else {
            return;
        };
        let Some(mask) = self.build_brush_mask(pixmap.width(), pixmap.height(), offset_x, offset_y)
        else {
            return;
        };
        pixmap.draw_pixmap(
            region.left as i32,
            region.top as i32,
            effect_pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            Some(&mask),
        );
    }

    /// 复制画笔矩形并生成模糊后的局部位图。
    fn build_blurred_region(&self, pixmap: &mut PixmapMut, region: &PixelRegion) -> Option<Pixmap> {
        let mut effect = Pixmap::new(region.width as u32, region.height as u32)?;
        let source_stride = pixmap.width() as usize * 4;
        let target_stride = region.width * 4;
        let source = pixmap.data_mut();
        let target = effect.data_mut();
        for row in 0..region.height {
            let source_start = (region.top + row) * source_stride + region.left * 4;
            let target_start = row * target_stride;
            target[target_start..target_start + target_stride]
                .copy_from_slice(&source[source_start..source_start + target_stride]);
        }
        BLUR_SCRATCH.with(|cell| {
            let mut scratch = cell.borrow_mut();
            let BlurScratch {
                region: pixels,
                blurred,
                current,
                scratch: work,
            } = &mut *scratch;
            pixels.clear();
            pixels.extend_from_slice(target);
            blurred.clear();
            blurred.resize(pixels.len(), 0);
            box_blur_rgba_with_scratch(
                pixels,
                blurred,
                region.width,
                region.height,
                self.radius.clamp(1, 50) as usize,
                current,
                work,
            );
            target.copy_from_slice(blurred);
        });
        Some(effect)
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
            let rect = tiny_skia::Rect::from_xywh(
                point.0 + offset_x - radius,
                point.1 + offset_y - radius,
                radius * 2.0,
                radius * 2.0,
            )?;
            mask_pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        } else {
            let mut builder = PathBuilder::new();
            builder.move_to(self.points[0].0 + offset_x, self.points[0].1 + offset_y);
            for point in &self.points[1..] {
                builder.line_to(point.0 + offset_x, point.1 + offset_y);
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
            self.radius = (width * 2.0).clamp(4.0, 50.0) as u32;
            self.brush_width = width.max(1.0);
        }
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        valid_rect(rect_from_tuple(self.rect))
    }

    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.rect.0 += dx;
        self.rect.1 += dy;
        for point in &mut self.points {
            point.0 += dx;
            point.1 += dy;
        }
    }

    pub(crate) fn resize_from_bounds(&mut self, from: Rect, to: Rect) {
        self.rect = tuple_from_rect(resize_rect(from, to, rect_from_tuple(self.rect)));
        for point in &mut self.points {
            *point = super::map_point(from, to, *point);
        }
    }
}

/// 使用调用方提供的缓冲执行盒式模糊，避免高频重放时反复分配。
/// 从目标画布按行复制矩形区域。
fn copy_rows_from_pixmap(pixmap: &mut PixmapMut, region: &PixelRegion, target: &mut [u8]) {
    let stride = pixmap.width() as usize * 4;
    let row_bytes = region.width * 4;
    let source = pixmap.data_mut();
    for row in 0..region.height {
        let start = (region.top + row) * stride + region.left * 4;
        let output = row * row_bytes;
        target[output..output + row_bytes].copy_from_slice(&source[start..start + row_bytes]);
    }
}

/// 将处理后的矩形区域按行写回目标画布。
fn copy_rows_to_pixmap(pixmap: &mut PixmapMut, region: &PixelRegion, source: &[u8]) {
    let stride = pixmap.width() as usize * 4;
    let row_bytes = region.width * 4;
    let target = pixmap.data_mut();
    for row in 0..region.height {
        let start = (region.top + row) * stride + region.left * 4;
        let input = row * row_bytes;
        target[start..start + row_bytes].copy_from_slice(&source[input..input + row_bytes]);
    }
}

fn box_blur_rgba_with_scratch(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    current: &mut Vec<u8>,
    scratch: &mut Vec<u8>,
) {
    debug_assert_eq!(src.len(), dst.len());
    if radius == 0 || width == 0 || height == 0 {
        dst.copy_from_slice(src);
        return;
    }

    current.clear();
    current.extend_from_slice(src);
    scratch.clear();
    scratch.resize(src.len(), 0);
    for _ in 0..2 {
        blur_pass(current, scratch, width, height, radius, true);
        blur_pass(scratch, current, width, height, radius, false);
    }
    dst.copy_from_slice(current);
}

/// 单方向盒式模糊；`horizontal` 为 false 时按列处理。
fn blur_pass(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    horizontal: bool,
) {
    if horizontal {
        for y in 0..height {
            blur_line(
                src,
                dst,
                y * width * 4, // 起始字节偏移
                width,
                4, // 步长：相邻像素间隔
                radius,
            );
        }
    } else {
        for x in 0..width {
            blur_line(src, dst, x * 4, height, width * 4, radius);
        }
    }
}

/// 在一个一维序列上滑动盒式窗口，`stride` 为相邻采样点的字节间隔。
fn blur_line(src: &[u8], dst: &mut [u8], start: usize, count: usize, stride: usize, radius: usize) {
    if count == 0 {
        return;
    }
    let mut sums = [0u32; 4];
    let mut left = 0usize;
    let mut right = radius.min(count - 1);
    for index in 0..=right {
        add_pixel(&mut sums, &src[start + index * stride..]);
    }

    for index in 0..count {
        let weight = (right - left + 1) as u32;
        let offset = start + index * stride;
        for channel in 0..4 {
            dst[offset + channel] = (sums[channel] / weight) as u8;
        }

        let next = index + 1;
        if next >= count {
            break;
        }
        let new_left = next.saturating_sub(radius);
        while left < new_left {
            subtract_pixel(&mut sums, &src[start + left * stride..]);
            left += 1;
        }
        let new_right = (next + radius).min(count - 1);
        while right < new_right {
            right += 1;
            add_pixel(&mut sums, &src[start + right * stride..]);
        }
    }
}

fn add_pixel(sums: &mut [u32; 4], pixel: &[u8]) {
    for channel in 0..4 {
        sums[channel] = sums[channel].saturating_add(u32::from(pixel[channel]));
    }
}

fn subtract_pixel(sums: &mut [u32; 4], pixel: &[u8]) {
    for channel in 0..4 {
        sums[channel] = sums[channel].saturating_sub(u32::from(pixel[channel]));
    }
}

/// 以零偏移绘制该图元，供测试与一次性回放使用。
impl BlurAction {
    pub fn draw(&self, pixmap: &mut tiny_skia::PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_blur_spreads_center_pixel() {
        let width = 11;
        let height = 11;
        let mut src = vec![255u8; width * height * 4];
        let center = ((5 * width) + 5) * 4;
        src[center] = 0;
        src[center + 1] = 0;
        src[center + 2] = 0;

        let mut dst = vec![0u8; src.len()];
        let mut current = Vec::new();
        let mut scratch = Vec::new();
        box_blur_rgba_with_scratch(&src, &mut dst, width, height, 2, &mut current, &mut scratch);

        assert!(
            dst[center] < 255,
            "中心黑点应被模糊扩散并拉低亮度，实际: {}",
            dst[center]
        );
        assert_eq!(dst[center + 3], 255, "模糊不得改变不透明度");
        let neighbor = ((5 * width) + 4) * 4;
        assert!(
            dst[neighbor] < 255,
            "中心黑点应向相邻像素扩散，实际: {}",
            dst[neighbor]
        );
        assert_eq!(&dst[0..4], &[255, 255, 255, 255][..]);
    }
}
