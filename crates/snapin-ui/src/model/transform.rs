//! 贴图窗口几何变换矩阵计算。
//!
//! 负责缩放因子限制、中心锚点缩放计算以及 90 度旋转与镜像翻转。

/// 贴图几何变换状态
///
/// 旋转与翻转直接作用于像素缓冲区，不再保存重复的角度/镜像标志，避免状态与画面脱节。
#[derive(Debug, Clone, PartialEq)]
pub struct PasterTransform {
    /// 缩放倍率 (0.1 ~ 10.0)
    scale: f32,
    /// 窗口不透明度 (0.1 ~ 1.0)
    opacity: f32,
}

impl Default for PasterTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            opacity: 1.0,
        }
    }
}

impl PasterTransform {
    /// 当前缩放倍率，始终位于 0.1 到 10.0。
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// 当前窗口不透明度，始终位于 0.1 到 1.0。
    pub const fn opacity(&self) -> f32 {
        self.opacity
    }

    /// 按照给定的滚轮角度增量调整缩放倍率，限制在 10% 到 1000% 之间
    pub fn zoom(&mut self, delta_y: f32) {
        if !delta_y.is_finite() || delta_y == 0.0 {
            return;
        }
        let factor = if delta_y > 0.0 { 1.1 } else { 0.9 };
        self.scale = (self.scale * factor).clamp(0.1, 10.0);
    }

    /// 精确设置缩放比例
    pub fn set_scale(&mut self, scale: f32) {
        if scale.is_finite() {
            self.scale = scale.clamp(0.1, 10.0);
        }
    }

    /// 调整不透明度
    pub fn adjust_opacity(&mut self, delta: f32) {
        if delta.is_finite() {
            self.opacity = (self.opacity + delta).clamp(0.1, 1.0);
        }
    }

    /// 精确设置不透明度，非有限值会被忽略。
    pub fn set_opacity(&mut self, opacity: f32) {
        if opacity.is_finite() {
            self.opacity = opacity.clamp(0.1, 1.0);
        }
    }
}

use tiny_skia::Pixmap;

/// 对 Pixmap 进行水平镜像翻转
pub fn flip_pixmap_horizontal(pixmap: &Pixmap) -> Option<Pixmap> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut out = Pixmap::new(w, h)?;
    let src = pixmap.data();
    let dst = out.data_mut();
    for y in 0..h {
        for x in 0..w {
            let src_idx = ((y * w + (w - 1 - x)) * 4) as usize;
            let dst_idx = ((y * w + x) * 4) as usize;
            dst[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
        }
    }
    Some(out)
}

/// 对 Pixmap 进行垂直镜像翻转
pub fn flip_pixmap_vertical(pixmap: &Pixmap) -> Option<Pixmap> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut out = Pixmap::new(w, h)?;
    let src = pixmap.data();
    let dst = out.data_mut();
    for y in 0..h {
        let src_row = (h - 1 - y) * w * 4;
        let dst_row = y * w * 4;
        dst[dst_row as usize..(dst_row + w * 4) as usize]
            .copy_from_slice(&src[src_row as usize..(src_row + w * 4) as usize]);
    }
    Some(out)
}

/// 将 Pixmap 顺时针旋转 90 度 (向右旋转)
pub fn rotate_pixmap_cw(pixmap: &Pixmap) -> Option<Pixmap> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut out = Pixmap::new(h, w)?;
    let src = pixmap.data();
    let dst = out.data_mut();
    for y in 0..h {
        for x in 0..w {
            let src_idx = ((y * w + x) * 4) as usize;
            let new_x = h - 1 - y;
            let new_y = x;
            let dst_idx = ((new_y * h + new_x) * 4) as usize;
            dst[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
        }
    }
    Some(out)
}

/// 将 Pixmap 逆时针旋转 90 度 (向左旋转)
pub fn rotate_pixmap_ccw(pixmap: &Pixmap) -> Option<Pixmap> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let mut out = Pixmap::new(h, w)?;
    let src = pixmap.data();
    let dst = out.data_mut();
    for y in 0..h {
        for x in 0..w {
            let src_idx = ((y * w + x) * 4) as usize;
            let new_x = y;
            let new_y = w - 1 - x;
            let dst_idx = ((new_y * h + new_x) * 4) as usize;
            dst[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_clamp() {
        let mut tf = PasterTransform::default();
        tf.zoom(100.0);
        assert!(tf.scale > 1.0);

        for _ in 0..100 {
            tf.zoom(100.0);
        }
        assert_eq!(tf.scale, 10.0);

        for _ in 0..100 {
            tf.zoom(-100.0);
        }
        assert_eq!(tf.scale, 0.1);
    }

    #[test]
    fn test_scale_and_opacity() {
        let mut transform = PasterTransform::default();
        transform.set_scale(0.5);
        assert_eq!(transform.scale, 0.5);

        transform.adjust_opacity(-5.0);
        assert_eq!(transform.opacity, 0.1);
        transform.adjust_opacity(5.0);
        assert_eq!(transform.opacity, 1.0);
    }

    #[test]
    fn test_zero_and_non_finite_zoom_delta_do_not_change_scale() {
        let mut transform = PasterTransform::default();
        let original = transform.scale;

        transform.zoom(0.0);
        assert_eq!(transform.scale, original);
        transform.zoom(f32::NAN);
        assert_eq!(transform.scale, original);
        transform.zoom(f32::INFINITY);
        assert_eq!(transform.scale, original);
    }

    #[test]
    fn test_non_finite_scale_and_opacity_delta_are_ignored() {
        let mut transform = PasterTransform::default();

        transform.set_scale(f32::NAN);
        transform.adjust_opacity(f32::INFINITY);

        assert_eq!(transform.scale, 1.0);
        assert_eq!(transform.opacity, 1.0);
    }

    #[test]
    fn test_pixmap_flip_and_rotate() {
        let mut pm = Pixmap::new(20, 10).unwrap();
        // 标记左上角像素为纯红色
        pm.data_mut()[0] = 255;
        pm.data_mut()[3] = 255;

        // 水平翻转：红色像素移到右上角 (x=19, y=0)
        let flipped_h = flip_pixmap_horizontal(&pm).unwrap();
        assert_eq!(flipped_h.width(), 20);
        assert_eq!(flipped_h.height(), 10);
        assert_eq!(flipped_h.data()[19 * 4], 255);

        // 垂直翻转：红色像素移到左下角 (x=0, y=9)
        let flipped_v = flip_pixmap_vertical(&pm).unwrap();
        assert_eq!(flipped_v.width(), 20);
        assert_eq!(flipped_v.height(), 10);
        assert_eq!(flipped_v.data()[9 * 20 * 4], 255);

        // 顺时针旋转 90 度：尺寸变为 10x20，原 (0, 0) 移至 (9, 0)
        let rotated_cw = rotate_pixmap_cw(&pm).unwrap();
        assert_eq!(rotated_cw.width(), 10);
        assert_eq!(rotated_cw.height(), 20);
        assert_eq!(rotated_cw.data()[9 * 4], 255);

        // 逆时针旋转 90 度：尺寸变为 10x20，原 (0, 0) 移至 (0, 19)
        let rotated_ccw = rotate_pixmap_ccw(&pm).unwrap();
        assert_eq!(rotated_ccw.width(), 10);
        assert_eq!(rotated_ccw.height(), 20);
        assert_eq!(rotated_ccw.data()[19 * 10 * 4], 255);
    }
}
