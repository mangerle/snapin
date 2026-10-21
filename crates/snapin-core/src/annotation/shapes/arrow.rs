//! 指引箭头标注图元绘制实现。

use crate::annotation::action::AnnotationAction;
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, PathBuilder, PixmapMut, Transform};

/// 指引箭头绘制动作
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArrowAction {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub color: (u8, u8, u8, u8),
    pub stroke_width: f32,
}

impl AnnotationAction for ArrowAction {
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        let (x1, y1) = (self.start.0 + offset_x, self.start.1 + offset_y);
        let (x2, y2) = (self.end.0 + offset_x, self.end.1 + offset_y);

        let dx = x2 - x1;
        let dy = y2 - y1;
        let length = (dx * dx + dy * dy).sqrt();
        if length < 2.0 {
            return;
        }

        // 单位方向向量与正交法向量
        let ux = dx / length;
        let uy = dy / length;
        let vx = -uy;
        let vy = ux;

        let half_stroke = (self.stroke_width / 2.0).max(1.0);
        let mut head_len = (self.stroke_width * 3.5).clamp(16.0, 80.0);
        let mut head_half_w = (self.stroke_width * 2.2).max(half_stroke + 4.0);

        // 短距离箭头等比平滑缩放，防止头部倒退吞噬箭身
        if length < head_len * 1.25 {
            let scale = (length / (head_len * 1.25)).max(0.2);
            head_len *= scale;
            head_half_w *= scale;
        }

        let base_x = x2 - ux * head_len;
        let base_y = y2 - uy * head_len;

        // 锥形箭杆：尾部收窄、向箭头方向线性加粗，形成毛笔式指向感。
        // 收窄比例固定在 32%，拖动距离越长锥度越明显，但不会细到亚像素丢失。
        let tail_half_w = (half_stroke * 0.32).max(0.8);

        // 7 顶点一体化多边形：保证尖端绝对锐利，彻底根除线头突出的几何畸变
        let mut pb = PathBuilder::new();
        // 1. 箭尾左侧
        pb.move_to(x1 + vx * tail_half_w, y1 + vy * tail_half_w);
        // 2. 箭身与头部交界左侧
        pb.line_to(base_x + vx * half_stroke, base_y + vy * half_stroke);
        // 3. 箭头左翼展端点
        pb.line_to(base_x + vx * head_half_w, base_y + vy * head_half_w);
        // 4. 箭头锐利尖端
        pb.line_to(x2, y2);
        // 5. 箭头右翼展端点
        pb.line_to(base_x - vx * head_half_w, base_y - vy * head_half_w);
        // 6. 箭身与头部交界右侧
        pb.line_to(base_x - vx * half_stroke, base_y - vy * half_stroke);
        // 7. 箭尾右侧
        pb.line_to(x1 - vx * tail_half_w, y1 - vy * tail_half_w);
        pb.close();

        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(self.color.0, self.color.1, self.color.2, self.color.3);
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    fn adjust_size(&mut self, stroke_w: f32, _font_sz: f32) {
        self.stroke_width = stroke_w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    #[test]
    fn test_arrow_drawing_and_size_adjustment() {
        let mut pixmap = Pixmap::new(200, 200).expect("分配 Pixmap 成功");
        let mut arrow = ArrowAction {
            start: (20.0, 20.0),
            end: (180.0, 180.0),
            color: (255, 0, 0, 255),
            stroke_width: 8.0,
        };

        arrow.draw(&mut pixmap.as_mut());

        // 终点尖端附近应被着色
        let tip_idx = ((175 * 200 + 175) * 4) as usize;
        assert_eq!(pixmap.data()[tip_idx], 255); // R
        assert_eq!(pixmap.data()[tip_idx + 3], 255); // A

        // 放大粗细后尖端几何仍有效
        arrow.adjust_size(24.0, 0.0);
        assert_eq!(arrow.stroke_width, 24.0);
        arrow.draw(&mut pixmap.as_mut());
    }

    /// 锥形箭杆：同一支箭头在尾部应明显窄于箭身中段（尾细头粗）
    #[test]
    fn test_arrow_shaft_tapers_toward_tail() {
        let mut pixmap = Pixmap::new(220, 80).expect("分配 Pixmap 成功");
        let arrow = ArrowAction {
            start: (20.0, 40.0),
            end: (200.0, 40.0),
            color: (255, 0, 0, 255),
            stroke_width: 12.0,
        };
        arrow.draw(&mut pixmap.as_mut());

        let column_height = |x: u32| {
            (0..80)
                .filter(|y| pixmap.pixel(x, *y).map(|p| p.alpha()) == Some(255))
                .count()
        };

        let tail = column_height(28);
        let middle = column_height(120);
        assert!(tail > 0, "箭尾应被着色，实际列高 {tail}");
        assert!(
            tail < middle,
            "尾部应窄于箭身中段，实际 尾={tail} 中={middle}"
        );
    }
}
