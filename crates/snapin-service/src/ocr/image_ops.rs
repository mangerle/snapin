//! OCR 与原位渲染专用图像几何处理与取色算法。
//!
//! 检测输入按 PaddleOCR 规范缩放到最长边 960 且宽高对齐 32 的倍数；
//! 识别输入按比例缩放到高度 48，并统一转换为 CHW 浮点张量。

use image::{Rgba, RgbaImage};

/// 检测模型输入最大边长（必须为 32 的倍数）
pub const DETECTION_MAX_SIDE: u32 = 960;
/// 检测模型输入宽高对齐粒度
pub const DETECTION_ALIGN: u32 = 32;
/// 识别模型输入固定高度
pub const RECOGNITION_HEIGHT: u32 = 48;
/// 识别模型输入最大宽度
pub const RECOGNITION_MAX_WIDTH: u32 = 1024;

const DETECTION_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DETECTION_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// 检测预处理结果
#[derive(Debug, Clone)]
pub struct DetectionInput {
    /// NCHW 扁平浮点张量
    pub chw: Vec<f32>,
    /// 模型输入宽度
    pub width: u32,
    /// 模型输入高度
    pub height: u32,
}

/// 图像预处理配置参数
#[derive(Debug, Clone, Copy)]
pub struct PreprocessParams {
    pub mean: [f32; 3],
    pub std: [f32; 3],
    pub target_height: u32,
}

impl Default for PreprocessParams {
    fn default() -> Self {
        Self {
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
            target_height: RECOGNITION_HEIGHT,
        }
    }
}

/// 将整张图像按检测模型要求缩放并转换为 CHW 浮点张量。
pub fn preprocess_detection_image(image: &RgbaImage) -> DetectionInput {
    let (orig_w, orig_h) = image.dimensions();
    if orig_w == 0 || orig_h == 0 {
        return DetectionInput {
            chw: Vec::new(),
            width: 0,
            height: 0,
        };
    }

    let (target_w, target_h) = detection_target_size(orig_w, orig_h);
    let resized = image::imageops::resize(
        image,
        target_w,
        target_h,
        image::imageops::FilterType::Triangle,
    );

    DetectionInput {
        chw: rgba_to_chw(&resized, DETECTION_MEAN, DETECTION_STD),
        width: target_w,
        height: target_h,
    }
}

/// 计算检测模型的目标输入尺寸：最长边不超过 960，且宽高均对齐到 32 的倍数。
fn detection_target_size(width: u32, height: u32) -> (u32, u32) {
    let max_side = DETECTION_MAX_SIDE;
    let scale = (max_side as f32 / width.max(height) as f32).min(1.0);
    let align_up = |value: u32| -> u32 {
        let aligned = value
            .max(DETECTION_ALIGN)
            .div_ceil(DETECTION_ALIGN)
            .saturating_mul(DETECTION_ALIGN);
        aligned.min(max_side - max_side % DETECTION_ALIGN)
    };

    let target_w = align_up((width as f32 * scale).round() as u32);
    let target_h = align_up((height as f32 * scale).round() as u32);
    (target_w.max(DETECTION_ALIGN), target_h.max(DETECTION_ALIGN))
}

/// 将单行文本裁剪图按比例缩放到固定高度，并转换为 CHW 浮点张量。
pub fn preprocess_recognition_image(
    image: &RgbaImage,
    params: &PreprocessParams,
) -> (Vec<f32>, u32, u32) {
    let (orig_w, orig_h) = image.dimensions();
    if orig_h == 0 || orig_w == 0 {
        return (Vec::new(), 0, 0);
    }

    let target_h = params.target_height.max(8);
    let ratio = orig_w as f32 / orig_h as f32;
    let target_w = ((target_h as f32 * ratio).round() as u32).clamp(16, RECOGNITION_MAX_WIDTH);

    let resized = image::imageops::resize(
        image,
        target_w,
        target_h,
        image::imageops::FilterType::Triangle,
    );

    (
        rgba_to_chw(&resized, params.mean, params.std),
        target_w,
        target_h,
    )
}

/// 将 RGBA 图像转换为归一化的 CHW 浮点张量。
fn rgba_to_chw(image: &RgbaImage, mean: [f32; 3], std: [f32; 3]) -> Vec<f32> {
    let (width, height) = image.dimensions();
    let Some(plane_size) = (width as usize).checked_mul(height as usize) else {
        return Vec::new();
    };
    let Some(total_size) = plane_size.checked_mul(3) else {
        return Vec::new();
    };
    if total_size == 0 {
        return Vec::new();
    }

    let mut chw = vec![0.0f32; total_size];
    for (index, pixel) in image.pixels().enumerate() {
        chw[index] = ((pixel[0] as f32 / 255.0) - mean[0]) / std[0];
        chw[plane_size + index] = ((pixel[1] as f32 / 255.0) - mean[1]) / std[1];
        chw[2 * plane_size + index] = ((pixel[2] as f32 / 255.0) - mean[2]) / std[2];
    }
    chw
}

/// 采样文本框外围环形边界像素，通过中位数滤波估计主导背景色 RGBA。
pub fn estimate_dominant_background_color(
    full_image: &RgbaImage,
    bbox: (f32, f32, f32, f32),
) -> (u8, u8, u8, u8) {
    let (img_w, img_h) = full_image.dimensions();
    if img_w == 0 || img_h == 0 {
        return (255, 255, 255, 255);
    }

    let (bx, by, bw, bh) = bbox;
    let x1 = (bx - 2.0).clamp(0.0, img_w as f32 - 1.0) as u32;
    let y1 = (by - 2.0).clamp(0.0, img_h as f32 - 1.0) as u32;
    let x2 = (bx + bw + 2.0).clamp(0.0, img_w as f32 - 1.0) as u32;
    let y2 = (by + bh + 2.0).clamp(0.0, img_h as f32 - 1.0) as u32;

    if x1 > x2 || y1 > y2 {
        return (255, 255, 255, 255);
    }

    let mut r_samples = Vec::with_capacity(128);
    let mut g_samples = Vec::with_capacity(128);
    let mut b_samples = Vec::with_capacity(128);

    // 仅采集外围边缘一圈的像素
    for x in x1..=x2 {
        for &y in &[y1, y2] {
            let Rgba([r, g, b, _]) = full_image.get_pixel(x, y);
            r_samples.push(*r);
            g_samples.push(*g);
            b_samples.push(*b);
        }
    }
    for y in y1..=y2 {
        for &x in &[x1, x2] {
            let Rgba([r, g, b, _]) = full_image.get_pixel(x, y);
            r_samples.push(*r);
            g_samples.push(*g);
            b_samples.push(*b);
        }
    }

    if r_samples.is_empty() {
        return (255, 255, 255, 255);
    }

    r_samples.sort_unstable();
    g_samples.sort_unstable();
    b_samples.sort_unstable();

    let mid = r_samples.len() / 2;
    (r_samples[mid], g_samples[mid], b_samples[mid], 255)
}

/// 计算颜色相对亮度并获取反差鲜明的文字推荐色。
pub fn get_contrasting_text_color(bg_color: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
    let (r, g, b, _) = bg_color;
    // WCAG 灰度亮度公式
    let lum = 0.299 * (r as f32) + 0.587 * (g as f32) + 0.114 * (b as f32);
    if lum > 128.0 {
        // 浅底 -> 配深炭黑字
        (24, 24, 27, 255)
    } else {
        // 深底 -> 配雪白字
        (245, 245, 245, 255)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detection_target_size_alignment() {
        assert_eq!(detection_target_size(640, 160), (640, 160));
        assert_eq!(detection_target_size(720, 200), (736, 224));
        let (w, h) = detection_target_size(3840, 2160);
        assert_eq!(w % DETECTION_ALIGN, 0);
        assert_eq!(h % DETECTION_ALIGN, 0);
        assert!(w <= DETECTION_MAX_SIDE && h <= DETECTION_MAX_SIDE);
    }

    #[test]
    fn test_preprocess_detection_returns_chw() {
        let image = RgbaImage::from_pixel(720, 200, Rgba([255, 255, 255, 255]));
        let input = preprocess_detection_image(&image);
        assert_eq!((input.width, input.height), (736, 224));
        assert_eq!(input.chw.len(), (3 * input.width * input.height) as usize);
    }

    #[test]
    fn test_preprocess_empty_image_is_safe() {
        let image = RgbaImage::new(0, 0);
        let input = preprocess_detection_image(&image);
        assert!(input.chw.is_empty());
        let (chw, width, height) =
            preprocess_recognition_image(&image, &PreprocessParams::default());
        assert!(chw.is_empty());
        assert_eq!((width, height), (0, 0));
    }

    #[test]
    fn test_estimate_dominant_background_clamps_out_of_bounds_bbox() {
        let image = RgbaImage::from_pixel(10, 10, Rgba([12, 34, 56, 255]));
        let color = estimate_dominant_background_color(&image, (100.0, 100.0, 50.0, 50.0));
        assert_eq!(color, (12, 34, 56, 255));
    }
}
