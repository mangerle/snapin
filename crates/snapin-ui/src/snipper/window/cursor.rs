//! 截图窗口光标形状计算与刷新。
//!
//! 光标形状完全由当前选区状态与悬停位置推导：拖拽中固定显示操作对应的形状，
//! 静止时按“工具栏 > 缩放手柄 > 选区内部”的优先级决定，避免拖拽过程中光标闪回箭头。

use super::annotation_edit::{self, AnnotationDrag};
use super::events::SnipperState;
use super::state::{LongCaptureAction, LongCaptureSession, SnipperPhase};
use crate::snipper::canvas::recording::{calculate_recording_layout, RecordingLayoutRequest};
use crate::snipper::selection::{hit_test_handle, DragMode, Rect, ResizeHandle, SelectionState};
use winit::window::{CursorIcon, Window};

/// 根据当前交互状态刷新窗口光标形状。
///
/// 只在期望形状变化时调用系统接口，避免每次鼠标移动都提交冗余的光标设置。
pub fn refresh_cursor(window: &Window, state: &mut SnipperState) {
    let icon = desired_cursor_icon(state);
    if state.viewport.cursor_icon != icon {
        state.viewport.cursor_icon = icon;
        window.set_cursor(icon);
    }
}

/// 计算当前应显示的光标形状。
fn desired_cursor_icon(state: &SnipperState) -> CursorIcon {
    match state.phase() {
        SnipperPhase::Busy => return CursorIcon::Wait,
        SnipperPhase::Idle | SnipperPhase::CreatingSelection => return CursorIcon::Default,
        _ => {}
    }

    let SelectionState::Locked { rect, active_drag } = &state.viewport.selection else {
        return CursorIcon::Default;
    };
    if state.recording.is_some() {
        return recording_cursor(state, rect);
    }
    if let Some(session) = state.long_capture.as_ref() {
        if !session.is_editing {
            return long_capture_cursor(state, session, rect, active_drag);
        }
    }
    let (cx, cy) = (
        state.viewport.cursor_pos.0 as f32,
        state.viewport.cursor_pos.1 as f32,
    );
    if let Some(icon) = active_drag_cursor(state, active_drag) {
        return icon;
    }
    if let Some(icon) = toolbar_cursor(state, cx, cy) {
        return icon;
    }
    if let Some(icon) = annotation_cursor(state, rect, cx, cy) {
        return icon;
    }
    selection_cursor(state, rect, cx, cy)
}

/// 录制阶段隐藏缩放手柄与普通工具栏交互，只在选区内部显示绘制光标。
fn recording_cursor(state: &SnipperState, rect: &Rect) -> CursorIcon {
    let (cx, cy) = (
        state.viewport.cursor_pos.0 as f32,
        state.viewport.cursor_pos.1 as f32,
    );
    let scale = state.editor.annotation.toolbar.ui_scale;
    if let Some(session) = state.recording.as_ref() {
        if session.is_dragging_toolbar() {
            return CursorIcon::Grabbing;
        }
        let layout = calculate_recording_layout(RecordingLayoutRequest {
            session,
            selection: rect,
            screen_width: state.viewport.screen_size.0,
            screen_height: state.viewport.screen_size.1,
            scale,
        });
        if layout.contains(cx, cy) {
            return if layout.hit(cx, cy)
                == Some(crate::snipper::window::state::RecordingItem::Action(
                    crate::snipper::window::state::RecordingAction::DragHandle,
                )) {
                CursorIcon::Grab
            } else {
                CursorIcon::Default
            };
        }
    }
    if rect.contains(cx, cy) && state.editor.annotation.toolbar.active_tool.is_some() {
        CursorIcon::Crosshair
    } else {
        CursorIcon::Default
    }
}

/// 拖拽过程中固定显示当前操作对应的光标，避免闪回箭头。
fn active_drag_cursor(state: &SnipperState, active_drag: &Option<DragMode>) -> Option<CursorIcon> {
    if let Some(drag) = &state.editor.annotation_drag {
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
    if state.editor.annotation.toolbar.is_dragging() {
        return Some(CursorIcon::Grabbing);
    }
    if state.editor.annotation.toolbar.hit_test_drag_handle(cx, cy) {
        return Some(CursorIcon::Grab);
    }
    state
        .editor
        .annotation
        .toolbar
        .contains_point(cx, cy)
        .then_some(CursorIcon::Default)
}

/// 标注手柄、标注主体与锁定旧标注的光标。
fn annotation_cursor(state: &SnipperState, rect: &Rect, sx: f32, sy: f32) -> Option<CursorIcon> {
    if !rect.contains(sx, sy) && state.long_canvas_space().is_some() {
        return None;
    }
    if !state.can_interact_annotation_at(sx, sy) {
        return None;
    }
    let (cx, cy) = state.annotation_point(sx, sy);
    if let Some(handle) = annotation_edit::hit_test_selected_handle(state, cx, cy) {
        return Some(resize_cursor(handle));
    }
    let allow_locked = state.viewport.modifiers.alt_key();
    if annotation_edit::hit_test_selected_body(state, cx, cy)
        || annotation_edit::hit_test_editable(state, (cx, cy), allow_locked).is_some()
    {
        return Some(CursorIcon::Move);
    }
    (state.editor.annotation.toolbar.active_tool.is_none()
        && annotation_edit::hit_test_locked(state, (cx, cy)))
    .then_some(CursorIcon::Default)
}

/// 长截图非编辑态专用光标：控制条拖拽把手显示抓握光标，仅在暂停且允许调整选区时显示缩放/移动光标。
fn long_capture_cursor(
    state: &SnipperState,
    session: &LongCaptureSession,
    rect: &Rect,
    active_drag: &Option<DragMode>,
) -> CursorIcon {
    if session.is_dragging_toolbar() {
        return CursorIcon::Grabbing;
    }
    if session.hovered_action == Some(LongCaptureAction::DragToolbar) {
        return CursorIcon::Grab;
    }
    let (cx, cy) = (
        state.viewport.cursor_pos.0 as f32,
        state.viewport.cursor_pos.1 as f32,
    );
    let in_bar = session.toolbar_rect.is_some_and(|r| r.contains(cx, cy))
        || session
            .crop_submenu_rect
            .is_some_and(|r| r.contains(cx, cy));
    if in_bar || !state.can_adjust_selection() {
        return CursorIcon::Default;
    }
    if let Some(drag) = active_drag {
        return match drag {
            DragMode::Move { .. } => CursorIcon::Move,
            DragMode::Resize(handle) => resize_cursor(*handle),
        };
    }
    let tolerance = 8.0 * state.editor.annotation.toolbar.ui_scale;
    if let Some(handle) = hit_test_handle(rect, cx, cy, tolerance) {
        return resize_cursor(handle);
    }
    if rect.contains(cx, cy) {
        return CursorIcon::Move;
    }
    CursorIcon::Default
}

/// 根据选区手柄、绘制工具与选区内部决定最终光标。
fn selection_cursor(state: &SnipperState, rect: &Rect, cx: f32, cy: f32) -> CursorIcon {
    let adjustable = state.can_adjust_selection();
    let tolerance = 8.0 * state.editor.annotation.toolbar.ui_scale;
    if adjustable {
        if let Some(handle) = hit_test_handle(rect, cx, cy, tolerance) {
            return resize_cursor(handle);
        }
    }

    if state.editor.annotation.toolbar.active_tool.is_some() {
        return if rect.contains(cx, cy) {
            CursorIcon::Crosshair
        } else {
            CursorIcon::Default
        };
    }
    if adjustable && rect.contains(cx, cy) {
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
        state.editor.annotation.toolbar.ui_scale = 1.0;
        state.viewport.selection = SelectionState::Locked {
            rect: Rect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 150.0,
            },
            active_drag: None,
        };

        state.viewport.cursor_pos = (300.0, 250.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::NwseResize);

        state.viewport.cursor_pos = (300.0, 175.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::EwResize);

        state.viewport.cursor_pos = (200.0, 175.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Move);

        state.viewport.cursor_pos = (50.0, 50.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);
    }

    /// 拖拽中光标必须锁定为操作形状，激活标注工具时不应显示选区拖拽光标
    #[test]
    fn test_cursor_icon_during_drag_and_with_active_tool() {
        let mut state = SnipperState::new();
        state.editor.annotation.toolbar.ui_scale = 1.0;
        state.viewport.cursor_pos = (200.0, 175.0);
        state.viewport.selection = SelectionState::Locked {
            rect: Rect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 150.0,
            },
            active_drag: Some(DragMode::Resize(ResizeHandle::BottomLeft)),
        };
        assert_eq!(desired_cursor_icon(&state), CursorIcon::NeswResize);

        state.viewport.selection = SelectionState::Locked {
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

        state.viewport.selection = SelectionState::Locked {
            rect: Rect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 150.0,
            },
            active_drag: None,
        };
        state.editor.annotation.toolbar.active_tool = Some(crate::editor::toolbar::Tool::Rect);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Crosshair);
    }

    /// 悬停已提交标注显示移动光标，选中后手柄显示方向缩放光标
    #[test]
    fn test_cursor_icon_for_annotation_editing() {
        use snapin_annotation::shapes::RectAction;
        use snapin_annotation::{AnnotationShape, LineStyle};

        let mut state = SnipperState::new();
        state.editor.annotation.toolbar.ui_scale = 1.0;
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
                rect: Rect::new(100.0, 100.0, 200.0, 120.0),
                color: (255, 0, 0, 255).into(),
                stroke_width: 2.0,
                is_filled: false,
                corner_radius: 0.0,
                line_style: LineStyle::Solid,
            }));

        state.viewport.cursor_pos = (150.0, 150.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Move);

        state.editor.selected_annotation = Some(0.into());
        state.viewport.cursor_pos = (300.0, 220.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::NwseResize);

        // 远离标注且移到选区外时恢复默认；选区内空白处保持移动光标
        state.viewport.cursor_pos = (600.0, 500.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Move);
        state.viewport.cursor_pos = (850.0, 650.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);
    }

    /// 长截图录制中、暂停态与编辑态必须展示隔离的光标反馈，禁止非法拖拽/缩放光标提示。
    #[test]
    fn test_desired_cursor_icon_isolates_long_capture_phases() {
        use crate::editor::toolbar::Tool;
        use crate::snipper::window::state::LongCaptureSession;
        use image::RgbaImage;

        let mut state = SnipperState::new();
        state.editor.annotation.toolbar.ui_scale = 1.0;
        let rect = Rect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 150.0,
        };
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        let mut session = LongCaptureSession::new(RgbaImage::new(200, 150));
        session.is_capturing = true;
        session.hovered_action = Some(LongCaptureAction::DragToolbar);
        state.long_capture = Some(session);

        // 1. 录制中：悬停拖拽把手为 Grab，手柄边角与选区内部均返回 Default
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Grab);
        if let Some(s) = state.long_capture.as_mut() {
            s.hovered_action = None;
        }
        state.viewport.cursor_pos = (300.0, 250.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);
        state.viewport.cursor_pos = (200.0, 175.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);

        // 2. 暂停态：恢复边界缩放手柄与内部移动光标
        if let Some(s) = state.long_capture.as_mut() {
            s.is_capturing = false;
        }
        state.viewport.cursor_pos = (300.0, 250.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::NwseResize);
        state.viewport.cursor_pos = (200.0, 175.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Move);

        // 3. 长图编辑态：边角与无工具内部均恢复 Default；激活工具时选区内部显示 Crosshair
        if let Some(s) = state.long_capture.as_mut() {
            s.enter_editing();
        }
        state.viewport.cursor_pos = (300.0, 250.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);
        state.viewport.cursor_pos = (200.0, 175.0);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Default);

        state.editor.annotation.toolbar.active_tool = Some(Tool::Rect);
        assert_eq!(desired_cursor_icon(&state), CursorIcon::Crosshair);
    }
}
