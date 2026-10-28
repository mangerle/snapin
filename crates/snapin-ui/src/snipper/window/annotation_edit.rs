//! 已提交标注的二次编辑：命中选中、整体拖动与包围盒手柄拉伸。

use super::drag_calculator::calculate_drag;
use super::events::SnipperState;
use crate::snipper::selection::{DragMode, Rect, ResizeHandle};
use snapin_core::annotation::{AnnotationShape, ShapeRect};

/// 标注编辑拖拽会话：始终基于按下瞬间的原始图元重建，避免逐帧累积误差
#[derive(Debug, Clone)]
pub enum AnnotationDrag {
    /// 整体移动
    Move {
        /// 按下位置，用于计算总位移
        origin: (f32, f32),
        original: AnnotationShape,
    },
    /// 包围盒手柄拉伸
    Resize {
        handle: ResizeHandle,
        /// 拖拽开始时的包围盒
        original_bounds: Rect,
        original: AnnotationShape,
    },
}

/// 将核心包围盒转为选区矩形类型
fn rect_from_bounds((x, y, width, height): ShapeRect) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// 将选区矩形转为核心包围盒类型
fn bounds_from_rect(rect: Rect) -> ShapeRect {
    (rect.x, rect.y, rect.width, rect.height)
}

/// 已选中标注的包围盒
pub fn selected_bounds(state: &SnipperState) -> Option<Rect> {
    let index = state.selected_annotation?;
    state.annotations.bounds_at(index).map(rect_from_bounds)
}

/// 已选中标注是否支持手柄拉伸
pub fn selected_supports_resize(state: &SnipperState) -> bool {
    state
        .selected_annotation
        .and_then(|index| state.annotations.shape_at(index))
        .is_some_and(AnnotationShape::supports_resize)
}

/// 命中已选中标注的缩放手柄
pub fn hit_test_selected_handle(state: &SnipperState, cx: f32, cy: f32) -> Option<ResizeHandle> {
    if !selected_supports_resize(state) {
        return None;
    }
    let rect = selected_bounds(state)?;
    rect.hit_test_handle(cx, cy, 8.0 * state.toolbar.ui_scale)
}

/// 处理左键按下：优先命中已选中标注的手柄，其次命中任意标注并开始整体拖动。
///
/// 该判断不受标注工具激活状态影响：画完一个标注后工具通常仍处于激活态，
/// 此时点击已有标注应当先选中并编辑，而不是在它上面叠画新图元。
/// 返回 true 表示事件已被标注编辑消费；点击空白返回 false 并取消当前选中，
/// 让选区移动、重新框选与继续绘制逻辑执行。
pub fn handle_press(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    if let Some(handle) = hit_test_selected_handle(state, cx, cy) {
        let Some(index) = state.selected_annotation else {
            return false;
        };
        let bounds = selected_bounds(state);
        let original = state.annotations.shape_at(index).cloned();
        if let (Some(original_bounds), Some(original)) = (bounds, original) {
            state.annotation_drag = Some(AnnotationDrag::Resize {
                handle,
                original_bounds,
                original,
            });
            return true;
        }
    }

    if let Some(index) = state.annotations.hit_test((cx, cy)) {
        state.selected_annotation = Some(index);
        if let Some(original) = state.annotations.shape_at(index).cloned() {
            state.annotation_drag = Some(AnnotationDrag::Move {
                origin: (cx, cy),
                original,
            });
        }
        return true;
    }

    clear(state);
    false
}

/// 处理标注拖拽过程中的鼠标移动；返回 true 表示画面已变化需要重绘。
pub fn handle_drag_move(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    let Some(index) = state.selected_annotation else {
        state.annotation_drag = None;
        return false;
    };
    let Some(drag) = state.annotation_drag.as_ref() else {
        return false;
    };

    let updated = match drag {
        AnnotationDrag::Move { origin, original } => {
            let mut shape = original.clone();
            shape.translate(cx - origin.0, cy - origin.1);
            shape
        }
        AnnotationDrag::Resize {
            handle,
            original_bounds,
            original,
        } => {
            let (new_bounds, _) =
                calculate_drag(original_bounds, &DragMode::Resize(*handle), (cx, cy));
            let mut shape = original.clone();
            shape.resize_from_bounds(
                bounds_from_rect(*original_bounds),
                bounds_from_rect(new_bounds),
            );
            shape
        }
    };

    if state.annotations.replace(index, updated) {
        true
    } else {
        clear(state);
        false
    }
}

/// 结束标注拖拽；返回 true 表示确实存在进行中的拖拽
pub fn handle_drag_release(state: &mut SnipperState) -> bool {
    state.annotation_drag.take().is_some()
}

/// 清除标注选中与拖拽状态
pub fn clear(state: &mut SnipperState) {
    state.selected_annotation = None;
    state.annotation_drag = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snipper::selection::SelectionState;
    use crate::snipper::toolbar::ToolbarItem;
    use snapin_core::annotation::shapes::RectAction;
    use snapin_core::annotation::LineStyle;

    fn state_with_rect() -> SnipperState {
        let mut state = SnipperState::new();
        state.toolbar.ui_scale = 1.0;
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            },
            active_drag: None,
        };
        state.annotations.push(AnnotationShape::Rect(RectAction {
            rect: (100.0, 100.0, 200.0, 120.0),
            color: (255, 0, 0, 255),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
        state
    }

    /// 点击标注本体应立即选中并进入整体拖动，位移只取决于按下点到当前光标的总差
    #[test]
    fn test_press_and_drag_moves_annotation() {
        let mut state = state_with_rect();

        assert!(handle_press(&mut state, 150.0, 150.0));
        assert_eq!(state.selected_annotation, Some(0));
        assert!(matches!(
            state.annotation_drag,
            Some(AnnotationDrag::Move { .. })
        ));

        assert!(handle_drag_move(&mut state, 180.0, 130.0));
        let bounds = state.annotations.bounds_at(0).expect("矩形必须有包围盒");
        assert_eq!(bounds.0, 130.0);
        assert_eq!(bounds.1, 80.0);

        assert!(handle_drag_release(&mut state));
        assert!(state.annotation_drag.is_none());
    }

    /// 已选中标注的手柄应进入拉伸模式，并按原始包围盒映射尺寸
    #[test]
    fn test_resize_handle_scales_annotation() {
        let mut state = state_with_rect();
        assert!(handle_press(&mut state, 150.0, 150.0));

        // 右下角手柄位于 (300, 220)，向外拖到 (400, 320)
        assert!(handle_press(&mut state, 300.0, 220.0));
        assert!(matches!(
            state.annotation_drag,
            Some(AnnotationDrag::Resize { .. })
        ));

        assert!(handle_drag_move(&mut state, 400.0, 320.0));
        let bounds = state.annotations.bounds_at(0).expect("矩形必须有包围盒");
        assert_eq!(bounds.0, 100.0);
        assert_eq!(bounds.1, 100.0);
        assert_eq!(bounds.2, 300.0);
        assert_eq!(bounds.3, 220.0);
    }

    /// 点击空白取消选中，且不消费事件（交还给选区逻辑）
    #[test]
    fn test_press_blank_clears_selection() {
        let mut state = state_with_rect();
        assert!(handle_press(&mut state, 150.0, 150.0));
        assert!(!handle_press(&mut state, 600.0, 500.0));
        assert!(state.selected_annotation.is_none());
        assert!(state.annotation_drag.is_none());
    }

    /// 标注工具激活时仍应允许直接点击已有标注进入编辑，而不是强制先取消工具
    #[test]
    fn test_active_tool_still_allows_annotation_editing() {
        let mut state = state_with_rect();
        state.toolbar.active_tool = Some(ToolbarItem::ToolPen);
        assert!(handle_press(&mut state, 150.0, 150.0));
        assert_eq!(state.selected_annotation, Some(0));
        assert!(matches!(
            state.annotation_drag,
            Some(AnnotationDrag::Move { .. })
        ));
    }
}
