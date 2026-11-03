//! 智能打码状态与逐行切换逻辑。
//!
//! OCR 完成后立即把全部文字区域作为马赛克图元写入标注历史，因此画布直接显示最终
//! 打码效果，不需要回车确认。`RedactionPreview` 只保留可点击文字区域，用户点击某行
//! 时精确增删对应马赛克图元，复制、保存与撤销重做始终使用同一份标注历史。

use super::state::SnipperState;
use crate::editor::toolbar::SubControl;
use crate::ports::OcrTextItem;
use crate::snipper::selection::Rect;
use snapin_annotation::shapes::MosaicAction;
use snapin_annotation::{AnnotationHistory, AnnotationShape};

/// 智能打码期间保留的文字区域；区域与实际马赛克图元一一对应。
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionPreview {
    /// OCR 文本行包围盒，全屏坐标系
    pub regions: Vec<Rect>,
}

impl RedactionPreview {
    /// 从 OCR 文本行构建区域列表，忽略零尺寸和无效包围盒。
    pub(crate) fn from_ocr_lines(lines: &[OcrTextItem]) -> Self {
        let mut regions = Vec::with_capacity(lines.len());
        for line in lines {
            if line.bounds.width > 1.0 && line.bounds.height > 1.0 {
                regions.push(line.bounds);
            }
        }
        Self { regions }
    }

    /// 返回点击位置命中的文字区域；重叠时优先处理最后一条。
    pub fn hit_at(&self, point: (f32, f32)) -> Option<Rect> {
        self.regions
            .iter()
            .rev()
            .find(|rect| rect.contains(point.0, point.1))
            .copied()
    }
}

/// 立即为全部有效文字区域添加马赛克，返回新增图元数量。
pub(crate) fn apply_redaction(state: &mut SnipperState, preview: &RedactionPreview) -> usize {
    sync_redaction_style(state, SubControl::EffectStrength);
    super::annotation_edit::clear(state);
    let mut count = 0;
    let effect_strength = state.editor.annotation.toolbar.sub_options.effect_strength;
    for rect in &preview.regions {
        if !is_redaction_applied(&state.editor.annotations, *rect) {
            state
                .editor
                .annotations
                .push(mosaic_shape(*rect, effect_strength));
            count += 1;
        }
    }
    count
}

/// 切换指定文字区域的马赛克状态；返回切换后是否已应用马赛克。
pub(crate) fn toggle_redaction_region(state: &mut SnipperState, rect: Rect) -> bool {
    if let Some(index) = redaction_mask_index(&state.editor.annotations, rect) {
        state.editor.annotations.remove_at(index);
        false
    } else {
        let effect_strength = state.editor.annotation.toolbar.sub_options.effect_strength;
        state
            .editor
            .annotations
            .push(mosaic_shape(rect, effect_strength));
        true
    }
}

/// 将尺寸或效果强度同步到本次智能打码的全部马赛克图元。
///
/// 智能打码会为每个 OCR 文字区域创建独立图元；若只调整最后一条，用户滚动滚轮时
/// 会出现同一批打码颗粒度不一致。因此尺寸与效果强度都按批同步，并收敛到同一颗粒度。
pub(crate) fn sync_redaction_style(state: &mut SnipperState, control: SubControl) -> bool {
    let toolbar = &mut state.editor.annotation.toolbar;
    if toolbar.active_tool != Some(crate::editor::toolbar::Tool::Mosaic)
        || toolbar.sub_options.mosaic_mode != crate::editor::toolbar::MosaicMode::SmartRedact
    {
        return false;
    }

    let cell_size = match control {
        SubControl::Size => {
            let cell_size = (toolbar.sub_options.custom_stroke_width * 2.0).clamp(4.0, 50.0);
            toolbar.sub_options.effect_strength = (cell_size * 4.0).round().clamp(0.0, 255.0) as u8;
            cell_size as u32
        }
        SubControl::EffectStrength => {
            let cell_size = (f32::from(toolbar.sub_options.effect_strength) / 4.0)
                .clamp(4.0, 50.0)
                .floor() as u32;
            toolbar
                .sub_options
                .set_custom_stroke_width(cell_size as f32 / 2.0);
            cell_size
        }
        _ => return false,
    };

    let Some(preview) = state.ocr.redaction_preview.as_ref() else {
        return true;
    };
    for rect in &preview.regions {
        let Some(index) = redaction_mask_index(&state.editor.annotations, *rect) else {
            continue;
        };
        let Some(mut shape) = state.editor.annotations.shape_at(index).cloned() else {
            continue;
        };
        let AnnotationShape::Mosaic(action) = &mut shape else {
            continue;
        };
        action.cell_size = cell_size;
        state.editor.annotations.replace(index, shape);
    }
    true
}

/// 判断指定文字区域当前是否已有对应马赛克图元。
pub(crate) fn is_redaction_applied(annotations: &AnnotationHistory, rect: Rect) -> bool {
    redaction_mask_index(annotations, rect).is_some()
}

fn redaction_mask_index(annotations: &AnnotationHistory, rect: Rect) -> Option<usize> {
    annotations
        .undo_stack()
        .iter()
        .rposition(|shape| matches_redaction_region(shape, rect))
}

fn matches_redaction_region(shape: &AnnotationShape, rect: Rect) -> bool {
    let AnnotationShape::Mosaic(action) = shape else {
        return false;
    };
    action.points.is_empty()
        && action.rect.x == rect.x
        && action.rect.y == rect.y
        && action.rect.width == rect.width
        && action.rect.height == rect.height
}

fn mosaic_shape(rect: Rect, effect_strength: u8) -> AnnotationShape {
    AnnotationShape::Mosaic(MosaicAction {
        rect,
        cell_size: (f32::from(effect_strength) / 4.0).clamp(4.0, 50.0) as u32,
        points: Vec::new(),
        brush_width: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f32, y: f32, width: f32, height: f32) -> OcrTextItem {
        OcrTextItem {
            text: "敏感文字".to_string(),
            bounds: Rect::new(x, y, width, height),
        }
    }

    /// 零尺寸 OCR 框不得进入区域列表，避免生成不可见图元。
    #[test]
    fn test_preview_filters_invalid_boxes() {
        let preview = RedactionPreview::from_ocr_lines(&[
            line(10.0, 20.0, 80.0, 20.0),
            line(0.0, 0.0, 0.0, 10.0),
        ]);
        assert_eq!(preview.regions.len(), 1);
    }

    /// 所有文字应立即写入马赛克，点击同一区域可实时取消并恢复。
    #[test]
    fn test_redaction_applies_immediately_and_toggles_region() {
        let mut state = SnipperState::new();
        let preview = RedactionPreview::from_ocr_lines(&[
            line(10.0, 20.0, 80.0, 20.0),
            line(10.0, 60.0, 80.0, 20.0),
        ]);

        assert_eq!(apply_redaction(&mut state, &preview), 2);
        assert!(preview
            .regions
            .iter()
            .all(|rect| is_redaction_applied(&state.editor.annotations, *rect)));

        let first = preview.regions[0];
        assert!(!toggle_redaction_region(&mut state, first));
        assert!(!is_redaction_applied(&state.editor.annotations, first));
        assert!(is_redaction_applied(
            &state.editor.annotations,
            preview.regions[1]
        ));

        assert!(toggle_redaction_region(&mut state, first));
        assert!(is_redaction_applied(&state.editor.annotations, first));
    }
}
