//! UI 几何契约：复用核心矩形并提供八方向缩放手柄命中。
//!
//! 矩形本身定义在 `snapin-foundation::geometry`，截图选区、工具栏布局、贴图输入区域与
//! 核心标注包围盒因此共享同一类型；UI 只保留与交互相关的缩放手柄语义。

pub use snapin_foundation::geometry::{Point, Rect};

/// 检测坐标是否命中矩形的 8 个控制手柄之一。
///
/// `tolerance` 为考虑 HiDPI 缩放下的热区扩张容差半径。
pub fn hit_test_handle(rect: &Rect, px: f32, py: f32, tolerance: f32) -> Option<ResizeHandle> {
    let r = tolerance.max(6.0);
    let handles = [
        (ResizeHandle::TopLeft, rect.x, rect.y),
        (ResizeHandle::Top, rect.x + rect.width / 2.0, rect.y),
        (ResizeHandle::TopRight, rect.x + rect.width, rect.y),
        (
            ResizeHandle::Right,
            rect.x + rect.width,
            rect.y + rect.height / 2.0,
        ),
        (
            ResizeHandle::BottomRight,
            rect.x + rect.width,
            rect.y + rect.height,
        ),
        (
            ResizeHandle::Bottom,
            rect.x + rect.width / 2.0,
            rect.y + rect.height,
        ),
        (ResizeHandle::BottomLeft, rect.x, rect.y + rect.height),
        (ResizeHandle::Left, rect.x, rect.y + rect.height / 2.0),
    ];

    for (handle, hx, hy) in handles {
        if (px - hx).abs() <= r && (py - hy).abs() <= r {
            return Some(handle);
        }
    }
    None
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
    fn test_hit_test_handle() {
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 150.0,
        };

        assert_eq!(
            hit_test_handle(&rect, 102.0, 99.0, 6.0),
            Some(ResizeHandle::TopLeft)
        );
        assert_eq!(
            hit_test_handle(&rect, 300.0, 250.0, 6.0),
            Some(ResizeHandle::BottomRight)
        );
        assert_eq!(
            hit_test_handle(&rect, 200.0, 100.0, 6.0),
            Some(ResizeHandle::Top)
        );
        assert_eq!(hit_test_handle(&rect, 200.0, 175.0, 6.0), None);
    }
}
