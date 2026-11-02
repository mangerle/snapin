//! 标注文档：有序图元集合与其查询、编辑能力。
//!
//! 文档只回答“当前画面上有哪些图元、命中哪个、包围盒多大”，不关心撤销/重做。
//! 历史管理器在此之上维护 redo 与提交语义，避免把“文档顺序”和“历史游标”
//! 混在同一个 `undo_stack` 里。

use super::action::AnnotationShape;
use crate::geometry::Rect;
use crate::shapes::SizeParam;
use serde::{Deserialize, Serialize};
use tiny_skia::PixmapMut;

/// 单次截图/贴图会话允许保留的最大图元数。
///
/// 这是防止长时间连续标注把内存拖到无上限的安全阀；正常使用远达不到。
/// 达到上限后丢弃最旧图元并告警，保留最新绘制结果。
pub const MAX_ANNOTATION_SHAPES: usize = 2_048;

/// 当前生效的标注图元集合，顺序即绘制层级。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AnnotationDocument {
    shapes: Vec<AnnotationShape>,
}

impl AnnotationDocument {
    /// 追加一个图元到文档顶层；超过安全上限时丢弃最旧图元并告警。
    pub(crate) fn push(&mut self, shape: AnnotationShape) {
        if self.shapes.len() >= MAX_ANNOTATION_SHAPES {
            let _ = self.shapes.remove(0);
            tracing::warn!(
                "标注图元数量达到上限 {}，已丢弃最旧图元以限制内存占用",
                MAX_ANNOTATION_SHAPES
            );
        }
        self.shapes.push(shape);
    }

    /// 弹出顶层图元，供撤销使用。
    pub(crate) fn pop(&mut self) -> Option<AnnotationShape> {
        self.shapes.pop()
    }

    /// 重新压回一个被撤销的图元。
    pub(crate) fn push_redo(&mut self, shape: AnnotationShape) {
        self.shapes.push(shape);
    }

    /// 当前图元数量。
    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    /// 是否没有任何图元。
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// 获取全部图元切片，顺序为自底向上。
    pub fn shapes(&self) -> &[AnnotationShape] {
        &self.shapes
    }

    /// 动态调节最近绘制的描边类图元粗细；最近图元不是描边类时不做任何修改。
    pub fn adjust_last_stroke_width(&mut self, stroke_width: f32) -> bool {
        let Some(last) = self.shapes.last_mut() else {
            return false;
        };
        match last {
            AnnotationShape::Rect(_)
            | AnnotationShape::Ellipse(_)
            | AnnotationShape::Arrow(_)
            | AnnotationShape::Pen(_)
            | AnnotationShape::Marker(_)
            | AnnotationShape::Mosaic(_)
            | AnnotationShape::Blur(_)
            | AnnotationShape::Badge(_) => {
                last.adjust_size(SizeParam::Stroke(stroke_width));
                true
            }
            _ => false,
        }
    }

    /// 动态调节最近绘制矩形的圆角半径；最近图元不是矩形时不做任何修改。
    pub fn adjust_last_corner_radius(&mut self, radius: f32) -> bool {
        let Some(last) = self.shapes.last_mut() else {
            return false;
        };
        if !matches!(last, AnnotationShape::Rect(_)) {
            return false;
        }
        last.adjust_corner_radius(radius);
        true
    }

    /// 动态调节最近绘制的文本/译文图元字号；最近图元不是文本类时不做任何修改。
    pub fn adjust_last_font_size(&mut self, font_size: f32) -> bool {
        let Some(last) = self.shapes.last_mut() else {
            return false;
        };
        match last {
            AnnotationShape::Text(_)
            | AnnotationShape::Translation(_)
            | AnnotationShape::Badge(_) => {
                last.adjust_size(SizeParam::Font(font_size));
                true
            }
            _ => false,
        }
    }

    /// 根据当前生效的徽章图元推导下一个可用序号。
    ///
    /// 序号基于文档中仍然存在的最大值计算，撤销、重做与替换都不会造成重复编号。
    pub fn next_badge_number(&self) -> u32 {
        let max_number = self
            .shapes
            .iter()
            .filter_map(|shape| match shape {
                AnnotationShape::Badge(badge) => Some(badge.number),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        u32::try_from(max_number)
            .unwrap_or(u32::MAX)
            .saturating_add(1)
    }

    /// 重新在目标 pixmap 上依次回放所有生效图元。
    pub fn replay_all(&self, pixmap: &mut PixmapMut) {
        self.replay_with_offset(pixmap, 0.0, 0.0);
    }

    /// 重新在目标局部 pixmap 上带平移偏移量回放所有生效图元。
    pub fn replay_with_offset(&self, pixmap: &mut PixmapMut, offset_x: f32, offset_y: f32) {
        for shape in &self.shapes {
            shape.draw_offset(pixmap, offset_x, offset_y);
        }
    }

    /// 从顶层向下命中测试，返回第一个包围盒包含该点的索引。
    pub fn hit_test(&self, point: (f32, f32)) -> Option<usize> {
        self.shapes
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, shape)| {
                let bounds = shape.bounds()?;
                bounds.contains(point.0, point.1).then_some(index)
            })
    }

    /// 获取指定索引图元的包围盒。
    pub fn bounds_at(&self, index: usize) -> Option<Rect> {
        self.shapes.get(index)?.bounds()
    }

    /// 获取指定索引图元的不可变借用。
    pub fn shape_at(&self, index: usize) -> Option<&AnnotationShape> {
        self.shapes.get(index)
    }

    /// 删除指定索引的图元并返回被删除项。
    pub(crate) fn remove_at(&mut self, index: usize) -> Option<AnnotationShape> {
        (index < self.shapes.len()).then(|| self.shapes.remove(index))
    }

    /// 删除所有包围盒与给定矩形相交的图元。
    pub(crate) fn remove_intersecting(&mut self, rect: Rect) -> Vec<AnnotationShape> {
        let mut removed = Vec::new();
        let mut index = self.shapes.len();
        while index > 0 {
            index -= 1;
            if self.shapes[index]
                .bounds()
                .is_some_and(|bounds| bounds.intersects(rect))
            {
                removed.push(self.shapes.remove(index));
            }
        }
        removed
    }

    /// 用编辑后的图元替换指定索引；越界返回 false。
    pub fn replace(&mut self, index: usize, shape: AnnotationShape) -> bool {
        let Some(slot) = self.shapes.get_mut(index) else {
            return false;
        };
        *slot = shape;
        true
    }

    /// 清空文档。
    pub fn clear(&mut self) {
        self.shapes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::RectAction;
    use crate::LineStyle;
    use tiny_skia::Pixmap;

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

    /// 文档可独立查询命中与包围盒，不依赖历史游标。
    #[test]
    fn test_document_hit_test_and_bounds() {
        let mut doc = AnnotationDocument::default();
        doc.push(rect_at(0.0));
        doc.push(rect_at(20.0));

        assert_eq!(doc.len(), 2);
        assert_eq!(doc.hit_test((5.0, 5.0)), Some(0));
        assert_eq!(doc.hit_test((25.0, 5.0)), Some(1));
        assert_eq!(doc.bounds_at(0).map(|rect| rect.x), Some(0.0));
    }

    /// 超出上限时只保留最新的图元，避免单会话无界增长。
    #[test]
    fn test_document_enforces_shape_limit() {
        let mut doc = AnnotationDocument::default();
        for index in 0..=MAX_ANNOTATION_SHAPES {
            doc.push(rect_at(index as f32));
        }
        assert_eq!(doc.len(), MAX_ANNOTATION_SHAPES);
        assert!(
            doc.shapes()[0].bounds().is_some_and(|rect| rect.x > 0.0),
            "超限后最旧图元应被丢弃"
        );
    }

    /// 文档序列化保持旧版 `[...]` 形状，兼容既有历史数据。
    #[test]
    fn test_document_serializes_as_shape_array() {
        let mut doc = AnnotationDocument::default();
        doc.push(rect_at(0.0));
        let json = serde_json::to_string(&doc).expect("文档应可序列化");
        assert!(json.starts_with('['), "文档应序列化为图元数组");
    }

    /// 空文档回放不得 panic。
    #[test]
    fn test_empty_document_replay_is_safe() {
        let doc = AnnotationDocument::default();
        let mut pixmap = Pixmap::new(4, 4).expect("创建测试位图失败");
        doc.replay_all(&mut pixmap.as_mut());
    }
}
