//! 贴图窗口几何变换矩阵计算。
//!
//! 负责缩放因子限制、中心锚点缩放计算以及 90 度旋转与镜像翻转。

/// 贴图几何变换状态
#[derive(Debug, Clone, PartialEq)]
pub struct PasterTransform {
    /// 缩放倍率 (0.1 ~ 10.0)
    pub scale: f32,
    /// 旋转角度 (0, 90, 180, 270)
    pub rotation_deg: u16,
    /// 水平镜像翻转
    pub flip_horizontal: bool,
    /// 垂直镜像翻转
    pub flip_vertical: bool,
    /// 窗口不透明度 (0.1 ~ 1.0)
    pub opacity: f32,
}

impl Default for PasterTransform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation_deg: 0,
            flip_horizontal: false,
            flip_vertical: false,
            opacity: 1.0,
        }
    }
}

impl PasterTransform {
    /// 按照给定的滚轮角度增量调整缩放倍率，限制在 10% 到 1000% 之间
    pub fn zoom(&mut self, delta_y: f32) {
        let factor = if delta_y > 0.0 { 1.1 } else { 0.9 };
        self.scale = (self.scale * factor).clamp(0.1, 10.0);
    }

    /// 顺时针旋转 90 度 (向右旋转)
    pub fn rotate_clockwise(&mut self) {
        self.rotation_deg = (self.rotation_deg + 90) % 360;
    }

    /// 逆时针旋转 90 度 (向左旋转)
    pub fn rotate_counter_clockwise(&mut self) {
        self.rotation_deg = (self.rotation_deg + 270) % 360;
    }

    /// 精确设置缩放比例
    pub fn set_scale(&mut self, s: f32) {
        self.scale = s.clamp(0.1, 10.0);
    }

    /// 水平翻转切换
    pub fn toggle_flip_horizontal(&mut self) {
        self.flip_horizontal = !self.flip_horizontal;
    }

    /// 垂直翻转切换
    pub fn toggle_flip_vertical(&mut self) {
        self.flip_vertical = !self.flip_vertical;
    }

    /// 调整不透明度
    pub fn adjust_opacity(&mut self, delta: f32) {
        self.opacity = (self.opacity + delta).clamp(0.1, 1.0);
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
    fn test_rotation() {
        let mut tf = PasterTransform::default();
        assert_eq!(tf.rotation_deg, 0);
        tf.rotate_clockwise();
        assert_eq!(tf.rotation_deg, 90);
        tf.rotate_clockwise();
        tf.rotate_clockwise();
        tf.rotate_clockwise();
        assert_eq!(tf.rotation_deg, 0);

        tf.rotate_counter_clockwise();
        assert_eq!(tf.rotation_deg, 270);
        tf.rotate_counter_clockwise();
        assert_eq!(tf.rotation_deg, 180);

        tf.set_scale(0.5);
        assert_eq!(tf.scale, 0.5);
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
