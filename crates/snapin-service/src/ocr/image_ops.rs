//! OCR 与原位渲染专用图像几何处理与取色算法。

use image::{Rgba, RgbaImage};

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
            target_height: 48,
        }
    }
}

/// 将局部 RGBA 图像按比例缩放到指定高度，并按 PaddleOCR 标准归一化转换为 CHW 扁平浮点数组
pub fn preprocess_recognition_image(
    image: &RgbaImage,
    params: &PreprocessParams,
) -> (Vec<f32>, u32, u32) {
    let (orig_w, orig_h) = image.dimensions();
    if orig_h == 0 || orig_w == 0 {
        return (Vec::new(), 0, 0);
    }

    let target_h = params.target_height;
    let ratio = orig_w as f32 / orig_h as f32;
    // 宽度向上取整并对齐为 8 的倍数
    let mut target_w = (target_h as f32 * ratio).round() as u32;
    target_w = target_w.max(target_h).min(1024);
    if !target_w.is_multiple_of(8) {
        target_w = ((target_w / 8) + 1) * 8;
    }

    let resized = image::imageops::resize(
        image,
        target_w,
        target_h,
        image::imageops::FilterType::Triangle,
    );

    let mut chw_tensor = vec![0.0f32; (3 * target_h * target_w) as usize];
    let plane_size = (target_h * target_w) as usize;

    for y in 0..target_h {
        for x in 0..target_w {
            let pixel = resized.get_pixel(x, y);
            let idx = (y * target_w + x) as usize;

            // R, G, B 通道归一化
            chw_tensor[idx] = ((pixel[0] as f32 / 255.0) - params.mean[0]) / params.std[0];
            chw_tensor[plane_size + idx] =
                ((pixel[1] as f32 / 255.0) - params.mean[1]) / params.std[1];
            chw_tensor[2 * plane_size + idx] =
                ((pixel[2] as f32 / 255.0) - params.mean[2]) / params.std[2];
        }
    }

    (chw_tensor, target_w, target_h)
}

/// 采样文本框外围环形边界像素，通过中位数滤波估计主导背景色 RGBA
pub fn estimate_dominant_background_color(
    full_image: &RgbaImage,
    bbox: (f32, f32, f32, f32),
) -> (u8, u8, u8, u8) {
    let (img_w, img_h) = full_image.dimensions();
    let (bx, by, bw, bh) = bbox;

    let x1 = (bx - 2.0).clamp(0.0, img_w as f32 - 1.0) as u32;
    let y1 = (by - 2.0).clamp(0.0, img_h as f32 - 1.0) as u32;
    let x2 = (bx + bw + 2.0).clamp(0.0, img_w as f32 - 1.0) as u32;
    let y2 = (by + bh + 2.0).clamp(0.0, img_h as f32 - 1.0) as u32;

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

/// 计算颜色相对亮度 (0.0 ~ 255.0) 并获取反差鲜明的文字推荐色
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
