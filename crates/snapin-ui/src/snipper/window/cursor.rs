//! 截图窗口光标形状计算与刷新。
//!
//! 光标形状完全由当前选区状态与悬停位置推导：拖拽中固定显示操作对应的形状，
//! 静止时按“工具栏 > 缩放手柄 > 选区内部”的优先级决定，避免拖拽过程中光标闪回箭头。

use super::annotation_edit::{self, AnnotationDrag};
use super::events::SnipperState;
use crate::snipper::selection::{hit_test_handle, DragMode, Rect, ResizeHandle, SelectionState};
use winit::window::{CursorIcon, Window};

/// 根据当前交互状态刷新窗口光标形状。
///
/// 只在期望形状变化时调用系统接口，避免每次鼠标移动都提交冗余的光标设置。
pub fn refresh_cursor(window: &Window, state: &mut SnipperState) {
    let icon = desired_cursor_icon(state);
    if state.cursor_icon != icon {
        state.cursor_icon = icon;
        window.set_cursor(icon);
    }
}

/// 计算当前应显示的光标形状。
fn desired_cursor_icon(state: &SnipperState) -> CursorIcon {
    let SelectionState::Locked { rect, active_drag } = &state.selection else {
        return CursorIcon::Default;
    };
    let (cx, cy) = (state.cursor_pos.0 as f32, state.cursor_pos.1 as f32);
    if let Some(icon) = active_drag_cursor(state, active_drag) {
        return icon;
    }
    if let Some(icon) = toolbar_cursor(state, cx, cy) {
        return icon;
    }
    if let Some(icon) = annotation_cursor(state, cx, cy) {
        return icon;
    }
    selection_cursor(state, rect, cx, cy)
}

/// 拖拽过程中固定显示当前操作对应的光标，避免闪回箭头。
fn active_drag_cursor(state: &SnipperState, active_drag: &Option<DragMode>) -> Option<CursorIcon> {
    if let Some(drag) = &state.annotation_drag {
        return Some(match drag {
            AnnotationDrag::Move { .. } => CursorIcon::Move,
            AnnotationDrag::Resize { handle, .. } => resize_cursor(*handle),
            AnnotationDrag::MagnifierSource { .. } | AnnotationDrag::MagnifierImage { .. } => {
                CursorIcon::Move
            }
        });
    }
    active_drag.as_ref().map(|drag| match drag {
        DragMode::Move { .. } => CursorIcon::Move,
        DragMode::Resize(handle) => resize_cursor(*handle),
    })
}

/// 工具栏拖动、拖动把手与面板区域的光标。
fn toolbar_cursor(state: &SnipperState, cx: f32, cy: f32) -> Option<CursorIcon> {
    if state.toolbar.is_dragging() {
        return Some(CursorIcon::Grabbing);
    }
    if state.toolbar.hit_test_drag_handle(cx, cy) {
        return Some(CursorIcon::Grab);
    }
    state
        .toolbar
        .contains_point(cx, cy)
        .then_some(CursorIcon::Default)
}

/// 标注手柄、标注主体与锁定旧标注的光标。
fn annotation_cursor(state: &SnipperState, cx: f32, cy: f32) -> Option<CursorIcon> {
    if let Some(handle) = annotation_edit::hit_test_selected_handle(state, cx, cy) {
        return Some(resize_cursor(handle));
    }
    let allow_locked = state.modifiers.alt_key();
    if annotation_edit::hit_test_selected_body(state, cx, cy)
        || annotation_edit::hit_test_editable(state, (cx, cy), allow_locked).is_some()
    {
        return Some(CursorIcon::Move);
    }
    (state.toolbar.active_tool.is_none() && annotation_edit::hit_test_locked(state, (cx, cy)))
        .then_some(CursorIcon::Default)
}

/// 根据选区手柄、绘制工具与选区内部决定最终光标。
fn selection_cursor(state: &SnipperState, rect: &Rect, cx: f32, cy: f32) -> CursorIcon {
    let tolerance = 8.0 * state.toolbar.ui_scale;
    if let Some(handle) = hit_test_handle(rect, cx, cy, tolerance) {
        return resize_cursor(handle);
    }

    if state.toolbar.active_tool.is_some() {
        return if rect.contains(cx, cy) {
            CursorIcon::Crosshair
        } else {
            CursorIcon::Default
        };
    }
    if rect.contains(cx, cy) {
        return CursorIcon::Move;
    }

    CursorIcon::Default
}

/// 将 8 向缩放手柄映射为系统光标形状。
fn resize_cursor(handle: ResizeHandle) -> CursorIcon {
    match handle {
        ResizeHandle::TopLeft | ResizeHandle::BottomRight => CursorIcon::NwseResize,
        ResizeHandle::TopRight | ResizeHandle::BottomLeft => CursorIcon::NeswResize,
        ResizeHandle::Top | ResizeHandle::Bottom => CursorIcon::NsResize,
        ResizeHandle::Left | ResizeHandle::Right => CursorIcon::EwResize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snipper::selection::Rect;

    /// 选区锁定后，手柄应显示方向缩放光标，内部显示移动光标，外部恢复默认
    #[test]
    fn test_cursor_icon_by_selection_region() {
        let mut state = SnipperState::new();
        state.toolbar.ui_scale = 1.0;
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 150.0,
            },
            active_drag: None,
        };

        state.cursor_pos = (300.0, 250.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::NwseResize);

        state.cursor_pos = (300.0, 175.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::EwResize);

        state.cursor_pos = (200.0, 175.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Move);

        state.cursor_pos = (50.0, 50.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);
    }

    /// 拖拽中光标必须锁定为操作形状，激活标注工具时不应显示选区拖拽光标
    #[test]
    fn test_cursor_icon_during_drag_and_with_active_tool() {
        let mut state = SnipperState::new();
        state.toolbar.ui_scale = 1.0;
        state.cursor_pos = (200.0, 175.0);
        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 150.0,
            },
            active_drag: Some(DragMode::Resize(ResizeHandle::BottomLeft)),
        };
        assert_eq!(desired_cursor_icon(&state), CursorIcon::NeswResize);

        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 150.0,
            },
            active_drag: Some(DragMode::Move {
                offset_x: 10,
                offset_y: 10,
            }),
        };
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Move);

        state.selection = SelectionState::Locked {
            rect: Rect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 150.0,
            },
            active_drag: None,
        };
        state.toolbar.active_tool = Some(crate::editor::toolbar::Tool::Rect);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Crosshair);
    }

    /// 悬停已提交标注显示移动光标，选中后手柄显示方向缩放光标
    #[test]
    fn test_cursor_icon_for_annotation_editing() {
        use snapin_annotation::shapes::RectAction;
        use snapin_annotation::{AnnotationShape, LineStyle};

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
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

        state.cursor_pos = (150.0, 150.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Move);

        state.selected_annotation = Some(0.into());
        state.cursor_pos = (300.0, 220.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::NwseResize);

        // 远离标注且移到选区外时恢复默认；选区内空白处保持移动光标
        state.cursor_pos = (600.0, 500.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Move);
        state.cursor_pos = (850.0, 650.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);
    }
}
