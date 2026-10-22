//! 局部模糊遮挡图元。
//!
//! 对指定矩形区域执行两轮可分离盒式模糊，近似高斯模糊效果。模糊在撤销/重做时
//! 从底图重新计算，而不是保存像素快照，因此历史栈保持轻量且结果确定。

use crate::annotation::action::AnnotationAction;
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

/// 局部模糊动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlurAction {
    pub rect: (f32, f32, f32, f32),
    /// 模糊半径（像素），实际计算时夹取到 1..=50
    pub radius: u32,
}

impl AnnotationAction for BlurAction {
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (rx, ry, rw, rh) = self.rect;
        let left = (rx + offset_x).max(0.0).floor() as usize;
        let top = (ry + offset_y).max(0.0).floor() as usize;
        let right = (rx + rw + offset_x).min(pixmap.width() as f32).ceil() as usize;
        let bottom = (ry + rh + offset_y).min(pixmap.height() as f32).ceil() as usize;

        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width == 0 || height == 0 {
            return;
        }

        let radius = self.radius.clamp(1, 50) as usize;
        let pixmap_width = pixmap.width() as usize;
        let row_bytes = width * 4;
        let mut region = Vec::with_capacity(row_bytes * height);
        {
            let data = pixmap.data_mut();
            for y in top..bottom {
                let start = (y * pixmap_width + left) * 4;
                region.extend_from_slice(&data[start..start + row_bytes]);
            }
        }

        let mut blurred = vec![0u8; region.len()];
        box_blur_rgba(&region, &mut blurred, width, height, radius);

        let data = pixmap.data_mut();
        for (row, y) in (top..bottom).enumerate() {
            let start = (y * pixmap_width + left) * 4;
            data[start..start + row_bytes]
                .copy_from_slice(&blurred[row * row_bytes..(row + 1) * row_bytes]);
        }
    }

    fn adjust_size(&mut self, stroke_w: f32, _font_sz: f32) {
        self.radius = (stroke_w * 2.0).clamp(4.0, 50.0) as u32;
    }
}

/// 对 RGBA 缓冲区执行两轮水平/垂直盒式模糊。
fn box_blur_rgba(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    debug_assert_eq!(src.len(), dst.len());
    if radius == 0 || width == 0 || height == 0 {
        dst.copy_from_slice(src);
        return;
    }

    let mut current = src.to_vec();
    let mut scratch = vec![0u8; src.len()];
    for _ in 0..2 {
        blur_pass(&current, &mut scratch, width, height, radius, true);
        blur_pass(&scratch, &mut current, width, height, radius, false);
    }
    dst.copy_from_slice(&current);
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
        box_blur_rgba(&src, &mut dst, width, height, 2);

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
