//! 核心共享几何契约。
//!
//! 标注包围盒、截图选区与 UI 布局共用同一个矩形类型，避免核心使用
//! `(f32, f32, f32, f32)` 元组、UI 再维护一套结构体而需要反复手工转换。

/// 轴对齐矩形 (x, y, width, height)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// 左上角 X 坐标
    pub x: f32,
    /// 左上角 Y 坐标
    pub y: f32,
    /// 宽度
    pub width: f32,
    /// 高度
    pub height: f32,
}

impl Rect {
    /// 根据两个对角点构建正规化的矩形（确保宽高非负）。
    pub fn from_points(p1: (f32, f32), p2: (f32, f32)) -> Self {
        let x = p1.0.min(p2.0);
        let y = p1.1.min(p2.1);
        Self {
            x,
            y,
            width: (p1.0 - p2.0).abs(),
            height: (p1.1 - p2.1).abs(),
        }
    }

    /// 判断某个坐标点是否落在矩形内部。
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    /// 整数像素对齐包围盒 (用于图像裁剪)。
    pub fn to_pixel_bounds(&self, max_w: u32, max_h: u32) -> (u32, u32, u32, u32) {
        let px = (self.x.max(0.0) as u32).min(max_w);
        let py = (self.y.max(0.0) as u32).min(max_h);
        let pw = (self.width as u32).min(max_w.saturating_sub(px));
        let ph = (self.height as u32).min(max_h.saturating_sub(py));
        (px, py, pw, ph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_normalization() {
        let rect = Rect::from_points((100.0, 200.0), (50.0, 80.0));
        assert_eq!(rect.x, 50.0);
        assert_eq!(rect.y, 80.0);
        assert_eq!(rect.width, 50.0);
        assert_eq!(rect.height, 120.0);
        assert!(rect.contains(60.0, 90.0));
        assert!(!rect.contains(10.0, 90.0));
    }

    #[test]
    fn test_pixel_bounds_clamps_to_image() {
        let rect = Rect {
            x: -10.0,
            y: 5.5,
            width: 500.0,
            height: 30.0,
        };
        assert_eq!(rect.to_pixel_bounds(100, 40), (0, 5, 100, 30));
    }
}
