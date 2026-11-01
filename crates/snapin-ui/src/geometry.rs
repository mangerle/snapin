//! UI 共享几何契约：矩形与八方向缩放手柄。
//!
//! 截图选区、工具栏布局、贴图输入区域与右键菜单都使用同一套矩形定义，
//! 避免各子系统各自维护互相转换的几何类型。

/// 矩形几何描述 (x, y, width, height)。
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
        let width = (p1.0 - p2.0).abs();
        let height = (p1.1 - p2.1).abs();
        Self {
            x,
            y,
            width,
            height,
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

    /// 检测坐标 (px, py) 是否命中 8 个控制手柄中的某一个。
    ///
    /// `tolerance` 为考虑 HiDPI 缩放下的热区扩张容差半径。
    pub fn hit_test_handle(&self, px: f32, py: f32, tolerance: f32) -> Option<ResizeHandle> {
        let r = tolerance.max(6.0);
        let handles = [
            (ResizeHandle::TopLeft, self.x, self.y),
            (ResizeHandle::Top, self.x + self.width / 2.0, self.y),
            (ResizeHandle::TopRight, self.x + self.width, self.y),
            (
                ResizeHandle::Right,
                self.x + self.width,
                self.y + self.height / 2.0,
            ),
            (
                ResizeHandle::BottomRight,
                self.x + self.width,
                self.y + self.height,
            ),
            (
                ResizeHandle::Bottom,
                self.x + self.width / 2.0,
                self.y + self.height,
            ),
            (ResizeHandle::BottomLeft, self.x, self.y + self.height),
            (ResizeHandle::Left, self.x, self.y + self.height / 2.0),
        ];

        for (handle, hx, hy) in handles {
            if (px - hx).abs() <= r && (py - hy).abs() <= r {
                return Some(handle);
            }
        }
        None
    }
}

/// 8 个边角控制手柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    /// 左上
    TopLeft,
    /// 上边中点
    Top,
    /// 右上
    TopRight,
    /// 右边中点
    Right,
    /// 右下
    BottomRight,
    /// 下边中点
    Bottom,
    /// 左下
    BottomLeft,
    /// 左边中点
    Left,
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
    fn test_hit_test_handle() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 150.0,
        };

        assert_eq!(
            rect.hit_test_handle(102.0, 99.0, 6.0),
            Some(ResizeHandle::TopLeft)
        );
        assert_eq!(
            rect.hit_test_handle(300.0, 250.0, 6.0),
            Some(ResizeHandle::BottomRight)
        );
        assert_eq!(
            rect.hit_test_handle(200.0, 100.0, 6.0),
            Some(ResizeHandle::Top)
        );
        assert_eq!(rect.hit_test_handle(200.0, 175.0, 6.0), None);
    }
}
