//! 标注撤销/重做历史。
//!
//! 历史由“当前文档 + 被撤销的图元栈”组成。绘制、撤销、重做只改变两者之间的
//! 图元归属；文档本身的查询与回放由 [`AnnotationDocument`] 负责，避免再次出现
//! 一个 `undo_stack` 同时充当图元数组与历史游标的双重身份。

use super::action::AnnotationShape;
use super::document::AnnotationDocument;
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

/// 标注历史管理器（文档 + redo 栈）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnotationHistory {
    /// 旧版持久化字段名保持 `undo_stack`，以兼容既有历史数据。
    #[serde(rename = "undo_stack", default)]
    document: AnnotationDocument,
    #[serde(default)]
    redo_stack: Vec<AnnotationShape>,
}

impl AnnotationHistory {
    /// 当前文档的只读视图。
    pub fn document(&self) -> &AnnotationDocument {
        &self.document
    }

    /// 添加一条新的标注操作，清空 redo 栈。
    pub fn push(&mut self, shape: AnnotationShape) {
        self.document.push(shape);
        self.redo_stack.clear();
    }

    /// 动态调节最近绘制的描边类图元粗细。
    pub fn adjust_last_stroke_width(&mut self, stroke_width: f32) -> bool {
        self.document.adjust_last_stroke_width(stroke_width)
    }

    /// 动态调节最近绘制矩形的圆角半径。
    pub fn adjust_last_corner_radius(&mut self, radius: f32) -> bool {
        self.document.adjust_last_corner_radius(radius)
    }

    /// 动态调节最近绘制的文本/译文图元字号。
    pub fn adjust_last_font_size(&mut self, font_size: f32) -> bool {
        self.document.adjust_last_font_size(font_size)
    }

    /// 根据当前文档推导下一个可用徽章序号。
    pub fn next_badge_number(&self) -> u32 {
        self.document.next_badge_number()
    }

    /// 撤销上一次标注。
    pub fn undo(&mut self) -> bool {
        if let Some(shape) = self.document.pop() {
            self.redo_stack.push(shape);
            true
        } else {
            false
        }
    }

    /// 恢复被撤销的标注。
    pub fn redo(&mut self) -> bool {
        if let Some(shape) = self.redo_stack.pop() {
            self.document.push_redo(shape);
            true
        } else {
            false
        }
    }

    /// 重新在目标 pixmap 上依次回放所有生效的标注图元。
    pub fn replay_all(&self, pixmap: &mut PixmapMut) {
        self.document.replay_all(pixmap);
    }

    /// 重新在目标局部 pixmap 上带平移偏移量回放所有生效的标注图元。
    pub fn replay_with_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        self.document.replay_with_offset(pixmap, offset_x, offset_y);
    }

    /// 当前是否有可撤销的操作。
    pub fn can_undo(&self) -> bool {
        !self.document.is_empty()
    }

    /// 当前是否有可重做的操作。
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// 获取当前文档图元切片，顺序为自底向上。
    pub fn undo_stack(&self) -> &[AnnotationShape] {
        self.document.shapes()
    }

    /// 从顶层向下命中测试标注图元。
    pub fn hit_test(&self, point: (f32, f32)) -> Option<usize> {
        self.document.hit_test(point)
    }

    /// 获取指定索引图元的包围盒。
    pub fn bounds_at(&self, index: usize) -> Option<crate::geometry::Rect> {
        self.document.bounds_at(index)
    }

    /// 获取指定索引图元的不可变借用。
    pub fn shape_at(&self, index: usize) -> Option<&AnnotationShape> {
        self.document.shape_at(index)
    }

    /// 用编辑后的图元替换指定索引；索引越界返回 false。
    ///
    /// 编辑不进入撤销栈：拖拽标注可能产生成百次中间状态，只有最终落定结果保留
    /// 在文档中，撤销语义仍是“撤销整次标注绘制”。任何编辑都会使 redo 失效，
    /// 避免把旧图元重做回已被修改的文档。
    pub fn replace(&mut self, index: usize, shape: AnnotationShape) -> bool {
        if self.document.replace(index, shape) {
            self.redo_stack.clear();
            true
        } else {
            false
        }
    }

    /// 删除指定索引的图元；成功时清空 redo 栈并返回是否删除。
    ///
    /// 智能打码需要按文字区域增删单个马赛克图元，不能误删与其相交的其他标注，
    /// 因此这里提供精确索引删除，而不是复用矩形相交擦除。
    pub fn remove_at(&mut self, index: usize) -> bool {
        self.take_at(index).is_some()
    }

    /// 移除并取出指定索引的图元；成功时清空 redo 栈。
    ///
    /// 用于对已提交标注（如文字标注）唤起就地重新编辑。
    pub fn take_at(&mut self, index: usize) -> Option<AnnotationShape> {
        let shape = self.document.remove_at(index);
        if shape.is_some() {
            self.redo_stack.clear();
        }
        shape
    }

    /// 清空所有标注历史。
    pub fn clear(&mut self) {
        self.document.clear();
        self.redo_stack.clear();
    }

    /// 擦除命中点最上层的图元。
    pub fn erase_at(&mut self, point: (f32, f32)) -> bool {
        let Some(index) = self.document.hit_test(point) else {
            return false;
        };
        self.document.remove_at(index).is_some()
    }

    /// 擦除所有与矩形相交的图元。
    pub fn erase_intersecting(&mut self, rect: crate::geometry::Rect) -> usize {
        self.document.remove_intersecting(rect).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::shapes::RectAction;
    use crate::LineStyle;

    fn rect_at(x: f32) -> AnnotationShape {
        AnnotationShape::Rect(RectAction {
            rect: Rect::new(x, 0.0, 10.0, 10.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 1.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        })
    }

    /// 撤销后编辑文档必须让 redo 失效，防止旧图元被重做到新文档。
    #[test]
    fn test_edit_invalidates_redo() {
        let mut history = AnnotationHistory::default();
        history.push(rect_at(0.0));
        history.push(rect_at(20.0));
        assert!(history.undo());
        assert!(history.can_redo());

        assert!(history.replace(0, rect_at(5.0)));
        assert!(!history.can_redo());
        assert!(!history.redo());
    }

    /// 精确删除指定图元后必须使 redo 失效，且不能影响其他标注。
    #[test]
    fn test_remove_at_removes_only_target_and_invalidates_redo() {
        let mut history = AnnotationHistory::default();
        history.push(rect_at(0.0));
        history.push(rect_at(20.0));
        history.undo();
        assert!(history.can_redo());

        assert!(history.remove_at(0));
        assert!(!history.can_redo());
        assert_eq!(history.undo_stack().len(), 0);
        assert!(!history.remove_at(0));
    }

    /// 旧版 `undo_stack` 字段仍可反序列化。
    #[test]
    fn test_legacy_undo_stack_field_is_accepted() {
        let legacy = r#"undo_stack = []"#;
        let history: AnnotationHistory = toml::from_str(legacy).expect("旧格式应可解析");
        assert!(history.undo_stack().is_empty());
    }
}
