//! 标注图元命令模式与撤销/重做控制。
//!
//! 将每一次绘制操作封装为具备 Execute 与 Undo 能力的 Command 对象，
//! 采用命令模式维护 Undo/Redo 历史栈。

use crate::annotation::shapes::{
    ArrowAction, BadgeAction, EllipseAction, MarkerAction, MosaicAction, PenAction, RectAction,
    TextAction,
};
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

/// 单次矢量标注行为的 Trait 抽象
pub trait AnnotationAction: Send + Sync {
    /// 执行带坐标偏移量的绘制（在全屏绘制时 offset 为 0，局部裁剪时 offset 为 -crop_x, -crop_y）
    fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32);

    /// 执行绘制（默认 offset 为 0.0）
    fn draw(&self, pixmap: &mut PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }

    /// 动态调节标注尺寸 (粗细或字体大小)
    fn adjust_size(&mut self, _stroke_w: f32, _font_sz: f32) {}
}

/// 所有支持的闭合矢量标注图元枚举 (代数数据类型，天然支持无堆分配存储、值拷贝与 Serde 序列化)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnnotationShape {
    Rect(RectAction),
    Ellipse(EllipseAction),
    Arrow(ArrowAction),
    Pen(PenAction),
    Marker(MarkerAction),
    Mosaic(MosaicAction),
    Badge(BadgeAction),
    Text(TextAction),
}

impl AnnotationShape {
    /// 执行带坐标偏移量的绘制
    pub fn draw_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        match self {
            Self::Rect(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Ellipse(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Arrow(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Pen(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Marker(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Mosaic(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Badge(a) => a.draw_offset(pixmap, offset_x, offset_y),
            Self::Text(a) => a.draw_offset(pixmap, offset_x, offset_y),
        }
    }

    /// 执行绘制（偏置量默认为 0.0）
    pub fn draw(&self, pixmap: &mut PixmapMut) {
        self.draw_offset(pixmap, 0.0, 0.0);
    }

    /// 动态调节图元尺寸 (粗细或字体大小)
    pub fn adjust_size(&mut self, stroke_w: f32, font_sz: f32) {
        match self {
            Self::Rect(a) => a.adjust_size(stroke_w, font_sz),
            Self::Ellipse(a) => a.adjust_size(stroke_w, font_sz),
            Self::Arrow(a) => a.adjust_size(stroke_w, font_sz),
            Self::Pen(a) => a.adjust_size(stroke_w, font_sz),
            Self::Marker(a) => a.adjust_size(stroke_w, font_sz),
            Self::Mosaic(a) => a.adjust_size(stroke_w, font_sz),
            Self::Badge(a) => a.adjust_size(stroke_w, font_sz),
            Self::Text(a) => a.adjust_size(stroke_w, font_sz),
        }
    }
}

/// 标注历史管理器 (Undo / Redo 栈)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnotationHistory {
    undo_stack: Vec<AnnotationShape>,
    redo_stack: Vec<AnnotationShape>,
}

impl AnnotationHistory {
    /// 添加一条新的标注操作，清空 redo 栈
    pub fn push(&mut self, shape: AnnotationShape) {
        self.undo_stack.push(shape);
        self.redo_stack.clear();
    }

    /// 动态调节历史栈顶图元（最近绘制的标注）的粗细或字体大小
    pub fn adjust_last_action_size(&mut self, stroke_w: f32, font_sz: f32) -> bool {
        if let Some(last) = self.undo_stack.last_mut() {
            last.adjust_size(stroke_w, font_sz);
            true
        } else {
            false
        }
    }

    /// 撤销上一次标注
    pub fn undo(&mut self) -> bool {
        if let Some(action) = self.undo_stack.pop() {
            self.redo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// 恢复被撤销的标注
    pub fn redo(&mut self) -> bool {
        if let Some(action) = self.redo_stack.pop() {
            self.undo_stack.push(action);
            true
        } else {
            false
        }
    }

    /// 重新在目标 pixmap 上依次回放所有生效的标注图元
    pub fn replay_all(&self, pixmap: &mut PixmapMut) {
        self.replay_with_offset(pixmap, 0.0, 0.0);
    }

    /// 重新在目标局部 pixmap 上带平移偏移量回放所有生效的标注图元
    pub fn replay_with_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        for action in &self.undo_stack {
            action.draw_offset(pixmap, offset_x, offset_y);
        }
    }

    /// 当前是否有可撤销的操作
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// 当前是否有可重做的操作
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 获取撤销栈切片借用
    pub fn undo_stack(&self) -> &[AnnotationShape] {
        &self.undo_stack
    }

    /// 清空所有标注历史
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_redo_stack() {
        let mut history = AnnotationHistory::default();
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        let rect_shape = AnnotationShape::Rect(RectAction {
            rect: (0.0, 0.0, 10.0, 10.0),
            color: (255, 0, 0, 255),
            stroke_width: 1.0,
            is_filled: false,
        });

        history.push(rect_shape);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        assert!(history.undo());
        assert!(!history.can_undo());
        assert!(history.can_redo());

        assert!(history.redo());
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_ellipse_and_badge_action() {
        use crate::annotation::shapes::{BadgeAction, EllipseAction};
        use tiny_skia::Pixmap;

        let mut pixmap = Pixmap::new(100, 100).unwrap();
        let mut mut_pix = pixmap.as_mut();

        let ellipse = EllipseAction {
            rect: (10.0, 10.0, 80.0, 60.0),
            color: (255, 0, 0, 255),
            stroke_width: 2.0,
            is_filled: false,
        };
        ellipse.draw(&mut mut_pix);

        let badge = BadgeAction {
            center: (50.0, 50.0),
            radius: 15.0,
            number: 1,
            color: (0, 128, 255, 255),
        };
        badge.draw(&mut mut_pix);

        // 验证气泡内部确实绘制出了纯白数字像素
        let data = pixmap.data();
        let has_white_digit = (40..60).any(|y| {
            (40..60).any(|x| {
                let idx = ((y * 100 + x) * 4) as usize;
                data[idx] > 200 && data[idx + 1] > 200 && data[idx + 2] > 200
            })
        });
        assert!(
            has_white_digit,
            "BadgeAction 必须在中心成功绘制出纯白色数字！"
        );
    }

    #[test]
    fn test_annotation_history_serde_roundtrip() {
        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Rect(RectAction {
            rect: (5.0, 5.0, 20.0, 20.0),
            color: (255, 0, 0, 255),
            stroke_width: 2.0,
            is_filled: false,
        }));

        let toml_str = toml::to_string(&history).expect("序列化 AnnotationHistory 成功");
        let deserialized: AnnotationHistory =
            toml::from_str(&toml_str).expect("反序列化 AnnotationHistory 成功");
        assert_eq!(history, deserialized);
    }

    #[test]
    fn test_replay_with_offset() {
        use tiny_skia::Pixmap;
        let mut history = AnnotationHistory::default();
        history.push(AnnotationShape::Rect(RectAction {
            rect: (100.0, 100.0, 50.0, 50.0),
            color: (0, 255, 0, 255),
            stroke_width: 1.0,
            is_filled: true,
        }));

        let mut pixmap = Pixmap::new(50, 50).unwrap();
        // 传入 -100, -100 偏置量，应正好绘制在 0..50 范围内
        history.replay_with_offset(&mut pixmap.as_mut(), -100.0, -100.0);

        let pixel = pixmap.pixel(25, 25).unwrap();
        assert_eq!(pixel.green(), 255);
        assert_eq!(pixel.alpha(), 255);
    }
}
