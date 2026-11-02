//! 已提交标注的二次编辑：命中选中、整体拖动与包围盒手柄拉伸。
//!
//! 交互约定：默认只有文档最新一条标注可被选中编辑，历史标注处于锁定态，
//! 让绘制工具可以继续在旧标注上方作画；按住 Alt 可显式解锁任意历史标注。

use super::drag_calculator::calculate_drag;
use super::events::SnipperState;
use crate::model::ShapeId;
use crate::snipper::selection::{hit_test_handle, DragMode, Rect, ResizeHandle};
use snapin_annotation::AnnotationShape;

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
    /// 拖动放大镜原区域，同时移动原区域与放大图像
    MagnifierSource {
        origin: (f32, f32),
        original: AnnotationShape,
    },
    /// 仅拖动放大镜图像
    MagnifierImage {
        origin: (f32, f32),
        original: AnnotationShape,
    },
}

/// 已选中标注的包围盒
pub fn selected_bounds(state: &SnipperState) -> Option<Rect> {
    let index = state.editor.selected_annotation?;
    state.editor.annotations.bounds_at(index.get())
}

/// 已选中标注是否支持手柄拉伸
pub fn selected_supports_resize(state: &SnipperState) -> bool {
    state
        .editor
        .selected_annotation
        .and_then(|index| state.editor.annotations.shape_at(index.get()))
        .is_some_and(AnnotationShape::supports_resize)
}

/// 命中已选中标注的缩放手柄
pub fn hit_test_selected_handle(state: &SnipperState, cx: f32, cy: f32) -> Option<ResizeHandle> {
    if !selected_supports_resize(state) {
        return None;
    }
    let rect = selected_bounds(state)?;
    hit_test_handle(&rect, cx, cy, 8.0 * state.editor.toolbar.ui_scale)
}

/// 命中当前允许编辑的标注：默认只允许最新一条，`allow_locked`（按住 Alt）时允许任意一条。
///
/// 设计原理：绘制工具激活时，历史图元若整块包围盒都可点击，用户几乎无法在其上方继续绘制。
/// 把默认可编辑范围收敛到最新一条，既保留"画完就能立刻微调"的体验，又让旧标注重新变成
/// 可绘制的画布；需要修改旧标注时用 Alt 显式解锁，避免误拖。
pub fn hit_test_editable(
    state: &SnipperState,
    point: (f32, f32),
    allow_locked: bool,
) -> Option<usize> {
    if allow_locked {
        return state.editor.annotations.hit_test(point);
    }
    let latest = state.editor.annotations.undo_stack().len().checked_sub(1)?;
    let bounds = state.editor.annotations.bounds_at(latest)?;
    bounds.contains(point.0, point.1).then_some(latest)
}

/// 点击位置是否命中了被锁定的历史标注（非最新一条，且未按 Alt 解锁）。
///
/// 用于给出"按住 Alt 可编辑"的提示，并避免无绘制工具时点击旧标注被误判为移动选区。
pub fn hit_test_locked(state: &SnipperState, point: (f32, f32)) -> bool {
    let Some(index) = state.editor.annotations.hit_test(point) else {
        return false;
    };
    hit_test_editable(state, point, false) != Some(index)
}

/// 点击是否落在当前选中标注的本体上；用于继续拖动已选中的旧标注。
pub fn hit_test_selected_body(state: &SnipperState, cx: f32, cy: f32) -> bool {
    selected_bounds(state).is_some_and(|rect| rect.contains(cx, cy))
}

/// 把选中项切换到当前最新图元；文档为空时清除选中。
///
/// 新画完、撤销与重做之后都调用它，保证"当前可编辑的标注"始终是文档最后一条。
pub fn select_latest(state: &mut SnipperState) {
    state.editor.annotation_drag = None;
    state.editor.selected_annotation = state
        .editor
        .annotations
        .undo_stack()
        .len()
        .checked_sub(1)
        .map(ShapeId::from);
}

/// 处理左键按下：优先命中已选中标注的手柄，其次命中当前允许编辑的标注并开始整体拖动。
///
/// 默认可编辑范围只有最新一条图元；按住 Alt 可显式选中任意历史图元。已经选中的标注
/// （包括通过 Alt 选中的旧图元）在后续按下时仍可直接拖动，否则每次微调都要重复按 Alt。
/// 返回 true 表示事件已被标注编辑消费；点击不可编辑区域返回 false 并取消当前选中，
/// 让选区移动、重新框选与继续绘制逻辑执行。
pub fn handle_press(state: &mut SnipperState, cx: f32, cy: f32, allow_locked: bool) -> bool {
    if try_start_handle_resize(state, cx, cy)
        || try_start_magnifier_drag(state, cx, cy)
        || try_start_selected_move(state, cx, cy)
    {
        return true;
    }

    if let Some(index) = hit_test_editable(state, (cx, cy), allow_locked) {
        state.editor.selected_annotation = Some(index.into());
        if let Some(original) = state.editor.annotations.shape_at(index).cloned() {
            state.editor.annotation_drag = Some(AnnotationDrag::Move {
                origin: (cx, cy),
                original,
            });
        }
        return true;
    }
    clear(state);
    false
}

/// 开始通过包围盒手柄拉伸已选标注。
fn try_start_handle_resize(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    let Some(handle) = hit_test_selected_handle(state, cx, cy) else {
        return false;
    };
    let Some(index) = state.editor.selected_annotation else {
        return false;
    };
    let Some(original_bounds) = selected_bounds(state) else {
        return false;
    };
    let Some(original) = state.editor.annotations.shape_at(index.get()).cloned() else {
        return false;
    };
    state.editor.annotation_drag = Some(AnnotationDrag::Resize {
        handle,
        original_bounds,
        original,
    });
    true
}

/// 放大镜的源区域和放大图可分别拖动。
fn try_start_magnifier_drag(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    let Some(index) = state.editor.selected_annotation else {
        return false;
    };
    let Some(original) = state.editor.annotations.shape_at(index.get()).cloned() else {
        return false;
    };
    let AnnotationShape::Magnifier(action) = &original else {
        return false;
    };
    let source = Rect {
        x: action.source.0,
        y: action.source.1,
        width: action.source.2,
        height: action.source.3,
    };
    let dest = Rect {
        x: action.dest.0,
        y: action.dest.1,
        width: action.dest.2,
        height: action.dest.3,
    };
    if source.contains(cx, cy) {
        state.editor.annotation_drag = Some(AnnotationDrag::MagnifierSource {
            origin: (cx, cy),
            original,
        });
        return true;
    }
    if dest.contains(cx, cy) {
        state.editor.annotation_drag = Some(AnnotationDrag::MagnifierImage {
            origin: (cx, cy),
            original,
        });
        return true;
    }
    false
}

/// 已选中的标注允许继续整体拖动，不要求重复按住 Alt。
fn try_start_selected_move(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    if !hit_test_selected_body(state, cx, cy) {
        return false;
    }
    let Some(index) = state.editor.selected_annotation else {
        return false;
    };
    let Some(original) = state.editor.annotations.shape_at(index.get()).cloned() else {
        return false;
    };
    state.editor.annotation_drag = Some(AnnotationDrag::Move {
        origin: (cx, cy),
        original,
    });
    true
}

/// 处理标注拖拽过程中的鼠标移动；返回 true 表示画面已变化需要重绘。
pub fn handle_drag_move(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    let Some(index) = state.editor.selected_annotation else {
        state.editor.annotation_drag = None;
        return false;
    };
    let Some(drag) = state.editor.annotation_drag.as_ref() else {
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
            shape.resize_from_bounds(*original_bounds, new_bounds);
            shape
        }
        AnnotationDrag::MagnifierSource { origin, original } => {
            let mut shape = original.clone();
            if let AnnotationShape::Magnifier(action) = &mut shape {
                action.translate_source(cx - origin.0, cy - origin.1);
            }
            shape
        }
        AnnotationDrag::MagnifierImage { origin, original } => {
            let mut shape = original.clone();
            if let AnnotationShape::Magnifier(action) = &mut shape {
                action.translate_dest(cx - origin.0, cy - origin.1);
            }
            shape
        }
    };

    if state.editor.annotations.replace(index.get(), updated) {
        true
    } else {
        clear(state);
        false
    }
}

/// 结束标注拖拽；返回 true 表示确实存在进行中的拖拽
pub fn handle_drag_release(state: &mut SnipperState) -> bool {
    state.editor.annotation_drag.take().is_some()
}

/// 清除标注选中与拖拽状态
pub fn clear(state: &mut SnipperState) {
    state.editor.selected_annotation = None;
    state.editor.annotation_drag = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::toolbar::Tool;
    use crate::snipper::selection::SelectionState;
    use snapin_annotation::shapes::{
        MagnifierAction, MagnifierConnector, MagnifierShape, RectAction,
    };
    use snapin_annotation::ColorRgba;
    use snapin_annotation::LineStyle;

    fn state_with_rect() -> SnipperState {
        let mut state = SnipperState::new();
        state.editor.toolbar.ui_scale = 1.0;
        state.viewport.selection = SelectionState::Locked {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 600.0,
            },
            active_drag: None,
        };
        state
            .editor
            .annotations
            .push(AnnotationShape::Rect(RectAction {
                rect: (100.0, 100.0, 200.0, 120.0),
                color: (255, 0, 0, 255).into(),
                stroke_width: 2.0,
                is_filled: false,
                corner_radius: 0.0,
                line_style: LineStyle::Solid,
            }));
        state
    }

    fn state_with_magnifier() -> SnipperState {
        let mut state = SnipperState::new();
        state.viewport.selection = SelectionState::Locked {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 300.0,
            },
            active_drag: None,
        };
        state
            .editor
            .annotations
            .push(AnnotationShape::Magnifier(MagnifierAction {
                source: (10.0, 10.0, 20.0, 20.0),
                dest: (100.0, 100.0, 40.0, 40.0),
                shape: MagnifierShape::Rectangle,
                border_width: 2.0,
                border_color: ColorRgba::opaque(255, 255, 255),
                smooth: true,
                shadow: true,
                connector: MagnifierConnector::Solid,
            }));
        state.editor.selected_annotation = Some(0.into());
        state
    }

    /// 拖动原区域时，原区域和放大图像同步移动。
    #[test]
    fn test_magnifier_source_drag_moves_both_regions() {
        let mut state = state_with_magnifier();
        assert!(handle_press(&mut state, 20.0, 20.0, false));
        assert!(matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::MagnifierSource { .. })
        ));
        assert!(handle_drag_move(&mut state, 30.0, 40.0));
        match state.editor.annotations.shape_at(0) {
            Some(AnnotationShape::Magnifier(action)) => {
                assert_eq!(action.source, (20.0, 30.0, 20.0, 20.0));
                assert_eq!(action.dest, (110.0, 120.0, 40.0, 40.0));
            }
            other => panic!("应为放大镜图元，实际: {other:?}"),
        }
    }

    /// 拖动放大图像区域时只移动图像，原区域保持不动。
    #[test]
    fn test_magnifier_image_drag_moves_only_output() {
        let mut state = state_with_magnifier();
        assert!(handle_press(&mut state, 110.0, 110.0, false));
        assert!(matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::MagnifierImage { .. })
        ));
        assert!(handle_drag_move(&mut state, 115.0, 117.0));
        match state.editor.annotations.shape_at(0) {
            Some(AnnotationShape::Magnifier(action)) => {
                assert_eq!(action.source, (10.0, 10.0, 20.0, 20.0));
                assert_eq!(action.dest, (105.0, 107.0, 40.0, 40.0));
            }
            other => panic!("应为放大镜图元，实际: {other:?}"),
        }
    }

    /// 点击标注本体应立即选中并进入整体拖动，位移只取决于按下点到当前光标的总差
    #[test]
    fn test_press_and_drag_moves_annotation() {
        let mut state = state_with_rect();

        assert!(handle_press(&mut state, 150.0, 150.0, false));
        assert_eq!(state.editor.selected_annotation, Some(0.into()));
        assert!(matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::Move { .. })
        ));

        assert!(handle_drag_move(&mut state, 180.0, 130.0));
        let bounds = state
            .editor
            .annotations
            .bounds_at(0)
            .expect("矩形必须有包围盒");
        assert_eq!(bounds.x, 130.0);
        assert_eq!(bounds.y, 80.0);

        assert!(handle_drag_release(&mut state));
        assert!(state.editor.annotation_drag.is_none());
    }

    /// 已选中标注的手柄应进入拉伸模式，并按原始包围盒映射尺寸
    #[test]
    fn test_resize_handle_scales_annotation() {
        let mut state = state_with_rect();
        assert!(handle_press(&mut state, 150.0, 150.0, false));

        // 右下角手柄位于 (300, 220)，向外拖到 (400, 320)
        assert!(handle_press(&mut state, 300.0, 220.0, false));
        assert!(matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::Resize { .. })
        ));

        assert!(handle_drag_move(&mut state, 400.0, 320.0));
        let bounds = state
            .editor
            .annotations
            .bounds_at(0)
            .expect("矩形必须有包围盒");
        assert_eq!(bounds.x, 100.0);
        assert_eq!(bounds.y, 100.0);
        assert_eq!(bounds.width, 300.0);
        assert_eq!(bounds.height, 220.0);
    }

    /// 点击空白取消选中，且不消费事件（交还给选区逻辑）
    #[test]
    fn test_press_blank_clears_selection() {
        let mut state = state_with_rect();
        assert!(handle_press(&mut state, 150.0, 150.0, false));
        assert!(!handle_press(&mut state, 600.0, 500.0, false));
        assert!(state.editor.selected_annotation.is_none());
        assert!(state.editor.annotation_drag.is_none());
    }

    /// 标注工具激活时仍应允许直接点击已有标注进入编辑，而不是强制先取消工具
    #[test]
    fn test_active_tool_still_allows_annotation_editing() {
        let mut state = state_with_rect();
        state.editor.toolbar.active_tool = Some(Tool::Pen);
        assert!(handle_press(&mut state, 150.0, 150.0, false));
        assert_eq!(state.editor.selected_annotation, Some(0.into()));
        assert!(matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::Move { .. })
        ));
    }

    /// 构造两个互不重叠的矩形，索引 1 为最新一条。
    fn state_with_two_rects() -> SnipperState {
        let mut state = state_with_rect();
        state
            .editor
            .annotations
            .push(AnnotationShape::Rect(RectAction {
                rect: (300.0, 300.0, 100.0, 100.0),
                color: (255, 0, 0, 255).into(),
                stroke_width: 2.0,
                is_filled: false,
                corner_radius: 0.0,
                line_style: LineStyle::Solid,
            }));
        state
    }

    /// 默认可编辑范围只有最新一条；旧标注必须按住 Alt 才能解锁。
    #[test]
    fn test_only_latest_annotation_is_editable_by_default() {
        let mut state = state_with_two_rects();

        // 最新的矩形可直接选中并进入拖动
        assert!(handle_press(&mut state, 350.0, 350.0, false));
        assert_eq!(state.editor.selected_annotation, Some(1.into()));
        assert!(matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::Move { .. })
        ));

        // 旧矩形默认锁定：不消费事件，也不留下选中状态
        clear(&mut state);
        assert!(!handle_press(&mut state, 150.0, 150.0, false));
        assert!(state.editor.selected_annotation.is_none());

        // 按住 Alt 才能显式解锁旧标注
        assert!(handle_press(&mut state, 150.0, 150.0, true));
        assert_eq!(state.editor.selected_annotation, Some(0.into()));
        assert!(matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::Move { .. })
        ));
    }

    /// Alt 选中的旧标注在后续按下时无需重复按住 Alt 即可继续拖动。
    #[test]
    fn test_selected_old_annotation_stays_draggable() {
        let mut state = state_with_two_rects();
        assert!(handle_press(&mut state, 150.0, 150.0, true));
        assert!(handle_drag_release(&mut state));

        assert!(handle_press(&mut state, 150.0, 150.0, false));
        assert_eq!(state.editor.selected_annotation, Some(0.into()));
        assert!(matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::Move { .. })
        ));
    }

    /// 历史变化后选中应重新落到文档最后一条；空文档则清除选中。
    #[test]
    fn test_select_latest_follows_document_tail() {
        let mut state = state_with_two_rects();
        select_latest(&mut state);
        assert_eq!(state.editor.selected_annotation, Some(1.into()));

        state.editor.annotations.clear();
        select_latest(&mut state);
        assert!(state.editor.selected_annotation.is_none());
    }
}
