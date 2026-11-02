//! 纯 Rust 二维码检测与解码。
//!
//! # 设计原理
//!
//! 二维码识别完全在本地内存中完成，不经过临时文件或网络服务，避免截图内容
//! 意外落盘。实现先把 RGBA 图像转换为灰度采样，再由 `rqrr` 检测多个二维码网格。
//! 一个选区允许包含多个码，因此接口返回集合而不是单个结果。
//!
//! # 代价与权衡
//!
//! 检测过程是同步 CPU 计算，调用方必须放到阻塞线程池。无法解码的候选网格会被
//! 跳过，以避免一个损坏二维码阻断同一图像中的其他有效二维码。

use image::RgbaImage;
use rqrr::PreparedImage;
use serde::{Deserialize, Serialize};

/// 单个二维码解码结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QrCodeResult {
    /// 解码后的文本内容
    pub text: String,
    /// 二维码在输入图像中的轴对齐包围盒 `[x, y, width, height]`
    pub bounds: (u32, u32, u32, u32),
}

/// 检测并解码图像中的全部二维码。
///
/// 空白图、模糊图和不含二维码的图像返回空集合；单个候选码解码失败不会影响
/// 其他候选码。
pub fn decode_qr_codes(image: &RgbaImage) -> Vec<QrCodeResult> {
    let width = image.width() as usize;
    let height = image.height() as usize;
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let raw = image.as_raw();
    let mut prepared = PreparedImage::prepare_from_greyscale(width, height, |x, y| {
        let index = (y * width + x) * 4;
        luma_from_rgba(&raw[index..index + 4])
    });

    let mut results = Vec::new();
    for grid in prepared.detect_grids() {
        let Ok((_metadata, text)) = grid.decode() else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        results.push(QrCodeResult {
            text,
            bounds: grid_bounds(&grid.bounds, image.width(), image.height()),
        });
    }
    results
}

/// 按 ITU-R BT.601 权重把 RGBA 转为灰度，忽略透明通道。
fn luma_from_rgba(pixel: &[u8]) -> u8 {
    let luma =
        0.299 * f32::from(pixel[0]) + 0.587 * f32::from(pixel[1]) + 0.114 * f32::from(pixel[2]);
    luma.round().clamp(0.0, 255.0) as u8
}

/// 把二维码四角坐标转换为夹取到图像范围内的轴对齐包围盒。
fn grid_bounds(points: &[rqrr::Point; 4], max_width: u32, max_height: u32) -> (u32, u32, u32, u32) {
    let min_x = points.iter().map(|point| point.x).min().unwrap_or(0);
    let min_y = points.iter().map(|point| point.y).min().unwrap_or(0);
    let max_x = points.iter().map(|point| point.x).max().unwrap_or(0);
    let max_y = points.iter().map(|point| point.y).max().unwrap_or(0);

    let left = min_x.clamp(0, i32::try_from(max_width).unwrap_or(i32::MAX)) as u32;
    let top = min_y.clamp(0, i32::try_from(max_height).unwrap_or(i32::MAX)) as u32;
    let right = max_x.clamp(0, i32::try_from(max_width).unwrap_or(i32::MAX)) as u32;
    let bottom = max_y.clamp(0, i32::try_from(max_height).unwrap_or(i32::MAX)) as u32;
    (
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// 空白图像不得误报二维码。
    #[test]
    fn test_blank_image_has_no_qr_code() {
        let image = RgbaImage::from_pixel(64, 64, Rgba([255, 255, 255, 255]));
        assert!(decode_qr_codes(&image).is_empty());
    }

    /// 灰度换算必须保持白底为白、黑块为黑，供检测器使用稳定阈值。
    #[test]
    fn test_luma_conversion_keeps_black_and_white() {
        assert_eq!(luma_from_rgba(&[255, 255, 255, 255]), 255);
        assert_eq!(luma_from_rgba(&[0, 0, 0, 255]), 0);
    }

    /// 生成真实二维码位图，验证检测、解码和坐标映射完整闭环。
    #[test]
    fn test_decode_generated_qr_code() {
        let code = qrcode::QrCode::new(b"https://example.com/snapin").expect("测试二维码生成成功");
        let modules = code.width();
        let scale = 8usize;
        let quiet = 4usize;
        let size = (modules + quiet * 2) * scale;
        let colors = code.to_colors();
        let mut image = RgbaImage::from_pixel(size as u32, size as u32, Rgba([255, 255, 255, 255]));
        for y in 0..modules {
            for x in 0..modules {
                if colors[y * modules + x] != qrcode::Color::Dark {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = ((x + quiet) * scale + dx) as u32;
                        let py = ((y + quiet) * scale + dy) as u32;
                        image.put_pixel(px, py, Rgba([0, 0, 0, 255]));
                    }
                }
            }
        }

        let results = decode_qr_codes(&image);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "https://example.com/snapin");
        assert!(results[0].bounds.2 > 0 && results[0].bounds.3 > 0);
    }
}
