//! 共享笔迹草稿状态：当前一笔的点集与拖拽标志。
//!
//! 截图侧与贴图侧此前各自维护 `pending_stroke_points` 与 `is_drawing`，坐标换算
//! 与提交规则容易分叉。本类型只承载“当前这一笔”的数据，具体坐标约束、样式
//! 预览与图元提交仍由两侧的会话逻辑完成。

/// 当前一笔标注草稿。
#[derive(Debug, Default)]
pub struct StrokeSession {
    /// 当前一笔的点集，坐标由调用方统一到图像像素
    pub points: Vec<(f32, f32)>,
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
    pub fn begin(&mut self, point: (f32, f32)) {
        self.points.clear();
        self.points.push(point);
        self.is_drawing = true;
    }

    /// 追加连续点（画笔/荧光笔）。
    pub fn append(&mut self, point: (f32, f32)) {
        if self.points.last() != Some(&point) {
            self.points.push(point);
        }
    }

    /// 更新矩形/箭头等两点图元的终点，保留起点。
    pub fn update_endpoint(&mut self, point: (f32, f32)) {
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
        stroke.begin((1.0, 2.0));
        stroke.append((3.0, 4.0));
        stroke.append((3.0, 4.0));
        assert_eq!(stroke.points, vec![(1.0, 2.0), (3.0, 4.0)]);
        assert!(stroke.is_drawing);

        stroke.update_endpoint((9.0, 9.0));
        assert_eq!(stroke.points, vec![(1.0, 2.0), (9.0, 9.0)]);

        stroke.begin((5.0, 5.0));
        assert_eq!(stroke.points, vec![(5.0, 5.0)]);

        stroke.clear();
        assert!(stroke.is_empty());
        assert!(!stroke.is_drawing);
    }
}
