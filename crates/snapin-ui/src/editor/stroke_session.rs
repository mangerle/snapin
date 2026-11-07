//! 共享笔迹草稿状态：当前一笔的点集与拖拽标志。
//!
//! 截图侧与贴图侧此前各自维护 `pending_stroke_points` 与 `is_drawing`，坐标换算
//! 与提交规则容易分叉。本类型只承载“当前这一笔”的数据，具体坐标约束、样式
//! 预览与图元提交仍由两侧的会话逻辑完成。

use crate::editor::toolbar::Tool;
use crate::geometry::Point;

/// 当前一笔标注草稿。
#[derive(Debug, Default)]
pub struct StrokeSession {
    /// 当前一笔的点集，坐标由调用方统一到图像像素
    pub points: Vec<Point>,
    /// 是否正处于拖拽绘制中
    pub is_drawing: bool,
}

impl StrokeSession {
    /// 创建带预分配容量的空笔迹。
    pub fn new() -> Self {
        Self {
            points: Vec::with_capacity(64),
            is_drawing: false,
        }
    }

    /// 开始一笔并记录起点。
    pub fn begin(&mut self, point: Point) {
        self.points.clear();
        self.points.push(point);
        self.is_drawing = true;
    }

    /// 追加连续点（画笔/荧光笔）。
    pub fn append(&mut self, point: Point) {
        if self.points.last() != Some(&point) {
            self.points.push(point);
        }
    }

    /// 更新矩形/箭头等两点图元的终点，保留起点。
    pub fn update_endpoint(&mut self, point: Point) {
        if self.points.len() > 1 {
            self.points[1] = point;
        } else {
            self.points.push(point);
        }
    }

    /// 清空当前笔迹并结束拖拽。
    pub fn clear(&mut self) {
        self.points.clear();
        self.is_drawing = false;
    }

    /// 按绘制工具的特征更新草稿点集：
    /// - 画笔/荧光笔/笔刷马赛克：连续追加点（内建去重）
    /// - 折线：由点击追加，光标移动时不改写已有折线点
    /// - 矩形/椭圆/单线/箭头等两点式图元：更新终点并保持起点不变
    pub fn update_with_tool(
        &mut self,
        tool: Tool,
        point: Point,
        is_brush_mosaic: bool,
        is_polyline: bool,
    ) {
        if !self.is_drawing {
            return;
        }
        match tool {
            Tool::Pen | Tool::Marker => self.append(point),
            Tool::Mosaic if is_brush_mosaic => self.append(point),
            Tool::Line if is_polyline => {}
            _ => self.update_endpoint(point),
        }
    }

    /// 是否没有任何待提交点。
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 开始新一笔必须清空旧点；画笔追加去重，矩形只更新终点。
    #[test]
    fn test_stroke_session_lifecycle() {
        let mut stroke = StrokeSession::new();
        stroke.begin(Point::new(1.0, 2.0));
        stroke.append(Point::new(3.0, 4.0));
        stroke.append(Point::new(3.0, 4.0));
        assert_eq!(
            stroke.points,
            vec![Point::new(1.0, 2.0), Point::new(3.0, 4.0)]
        );
        assert!(stroke.is_drawing);

        stroke.update_endpoint(Point::new(9.0, 9.0));
        assert_eq!(
            stroke.points,
            vec![Point::new(1.0, 2.0), Point::new(9.0, 9.0)]
        );

        stroke.begin(Point::new(5.0, 5.0));
        assert_eq!(stroke.points, vec![Point::new(5.0, 5.0)]);

        stroke.clear();
        assert!(stroke.is_empty());
        assert!(!stroke.is_drawing);
    }
}
