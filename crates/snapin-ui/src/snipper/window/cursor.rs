//! 截图窗口光标形状计算与刷新。
//!
//! 光标形状完全由当前选区状态与悬停位置推导：拖拽中固定显示操作对应的形状，
//! 静止时按“工具栏 > 缩放手柄 > 选区内部”的优先级决定，避免拖拽过程中光标闪回箭头。

use super::events::SnipperState;
use crate::snipper::selection::{DragMode, ResizeHandle, SelectionState};
use winit::window::{CursorIcon, Window};

/// 根据当前交互状态刷新窗口光标形状。
///
/// 只在期望形状变化时调用系统接口，避免每次鼠标移动都提交冗余的光标设置。
pub fn refresh_cursor(window: &Window, state: &mut SnipperState) {
    let icon = desired_cursor_icon(state);
    if state.cursor_icon != icon {
        state.cursor_icon = icon;
        window.set_cursor_icon(icon);
    }
}

/// 计算当前应显示的光标形状。
fn desired_cursor_icon(state: &SnipperState) -> CursorIcon {
    let SelectionState::Locked { rect, active_drag } = &state.selection else {
        return CursorIcon::Default;
    };

    // 拖拽中始终显示当前操作对应的形状，避免光标在拖动过程中闪回箭头
    if let Some(drag) = active_drag {
        return match drag {
            DragMode::Move { .. } => CursorIcon::Move,
            DragMode::Resize(handle) => resize_cursor(*handle),
        };
    }

    let (cx, cy) = (state.cursor_pos.0 as f32, state.cursor_pos.1 as f32);
    if state.toolbar.active_tool.is_some() || state.toolbar.contains_point(cx, cy) {
        return CursorIcon::Default;
    }

    let tolerance = 8.0 * state.toolbar.ui_scale;
    if let Some(handle) = rect.hit_test_handle(cx, cy, tolerance) {
        return resize_cursor(handle);
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
        state.toolbar.active_tool = Some(crate::snipper::toolbar::ToolbarItem::ToolRect);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);
    }
}
