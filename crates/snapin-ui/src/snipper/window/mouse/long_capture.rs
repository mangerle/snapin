//! 长截图控制条点击与选区首帧提取。

use super::super::types::SnipperOutcome;
use super::*;
use crate::geometry::Point;
use crate::snipper::window::state::LongCaptureAction;
use snapin_render::stitching::StitchDirection;
use tracing::info;

/// 处理长截图控制条按钮点击。
pub(super) fn handle_long_capture_press(
    state: &mut SnipperState,
    pointer: Point,
    original_rgba: &RgbaImage,
) -> bool {
    let ui_scale = state.editor.annotation.toolbar.ui_scale;
    let (sel_w, sel_h) = match state.viewport.selection {
        SelectionState::Locked { rect, .. } => {
            (rect.width.round() as u32, rect.height.round() as u32)
        }
        _ => (0, 0),
    };

    let action = state.long_capture.as_ref().and_then(|session| {
        crate::snipper::canvas::long_capture::hit_test_long_capture_bar(
            session, pointer.x, pointer.y, ui_scale, sel_w, sel_h,
        )
    });

    let Some(action) = action else {
        return true;
    };

    info!("执行长截图操作: {}", action.label());

    match action {
        LongCaptureAction::DragToolbar => {
            if let Some(session) = state.long_capture.as_mut() {
                session.begin_toolbar_drag(pointer.x, pointer.y);
            }
        }
        LongCaptureAction::ToggleCapture => {
            let is_capturing = state
                .long_capture
                .as_ref()
                .is_some_and(|session| session.is_capturing);
            if is_capturing {
                if let Some(session) = state.long_capture.as_mut() {
                    session.toggle_capture();
                }
                state.set_status_message(snapin_foundation::i18n::tr(
                    "长截图已停止：可调整选区或裁剪，点击开始重新录制",
                ));
            } else if let Some(first_frame) = current_selection_frame(state, original_rgba) {
                if let SelectionState::Locked { active_drag, .. } = &mut state.viewport.selection {
                    *active_drag = None;
                }
                state.editor.annotation.active_text_input = None;
                state.editor.annotation.is_selecting_text = false;
                state.editor.annotation.stroke.clear();
                state.editor.annotation.toolbar.active_tool = None;
                crate::snipper::window::annotation_edit::clear(state);
                if let Some(session) = state.long_capture.as_mut() {
                    session.crop_sub_menu_open = false;
                    session.start_capture(first_frame);
                }
                state.set_status_message(snapin_foundation::i18n::tr(
                    "长截图录制中：在选区内滚动鼠标自动拼接",
                ));
            } else {
                state.set_status_message(snapin_foundation::i18n::tr(
                    "选区过小，无法开始长截图录制",
                ));
            }
        }
        LongCaptureAction::ToggleDirection => {
            if let Some(session) = state.long_capture.as_mut() {
                session.toggle_direction();
                state.set_status_message(snapin_foundation::i18n::tr(
                    "已切换长截图主方向并重置本轮拼接",
                ));
            }
        }
        LongCaptureAction::ToggleCropSubMenu => {
            if let Some(session) = state.long_capture.as_mut() {
                session.toggle_crop_sub_menu();
            }
        }
        LongCaptureAction::ToggleAutoCrop => {
            if let Some(session) = state.long_capture.as_mut() {
                session.toggle_auto_crop();
                let msg = if session.auto_crop {
                    "已开启反向滚动自动裁剪"
                } else {
                    "已关闭反向滚动自动裁剪"
                };
                state.set_status_message(snapin_foundation::i18n::tr(msg));
            }
        }
        LongCaptureAction::CropHead => {
            if let Some(session) = state.long_capture.as_mut() {
                session.crop_head(30);
                let msg = match session.direction {
                    StitchDirection::Vertical => "已执行上裁剪 (30px)",
                    StitchDirection::Horizontal => "已执行左裁剪 (30px)",
                };
                state.set_status_message(snapin_foundation::i18n::tr(msg));
            }
        }
        LongCaptureAction::CropTail => {
            if let Some(session) = state.long_capture.as_mut() {
                session.crop_tail(30);
                let msg = match session.direction {
                    StitchDirection::Vertical => "已执行下裁剪 (30px)",
                    StitchDirection::Horizontal => "已执行右裁剪 (30px)",
                };
                state.set_status_message(snapin_foundation::i18n::tr(msg));
            }
        }
        LongCaptureAction::EnterEditing => {
            if let Some(session) = state.long_capture.as_mut() {
                session.enter_editing();
                if let SelectionState::Locked {
                    rect, active_drag, ..
                } = &mut state.viewport.selection
                {
                    *active_drag = None;
                    state
                        .editor
                        .annotation
                        .toolbar
                        .configure_for_long_capture_editing();
                    state.editor.annotation.toolbar.update_layout(
                        rect,
                        state.viewport.screen_size.0,
                        state.viewport.screen_size.1,
                    );
                } else {
                    state
                        .editor
                        .annotation
                        .toolbar
                        .configure_for_long_capture_editing();
                }
                state.set_status_message(snapin_foundation::i18n::tr(
                    "已进入长图编辑框：滚轮滚动浏览长图，可使用标注工具",
                ));
            }
        }
        LongCaptureAction::Pin => {
            crate::snipper::window::actions::handle_action_pin(state, original_rgba);
        }
        LongCaptureAction::Save => {
            crate::snipper::window::actions::handle_action_save(state, original_rgba);
        }
        LongCaptureAction::Copy => {
            crate::snipper::window::actions::handle_action_copy(state, original_rgba);
        }
        LongCaptureAction::Close => {
            state.abort_long_capture();
            state.viewport.selection = SelectionState::Idle;
            state.session.outcome = Some(SnipperOutcome::Cancelled);
        }
    }
    true
}

/// 从完整屏幕快照裁剪出当前锁定选区，供新一轮长截图录制初始化拼接器。
fn current_selection_frame(state: &SnipperState, original_rgba: &RgbaImage) -> Option<RgbaImage> {
    let SelectionState::Locked { rect, .. } = state.viewport.selection else {
        return None;
    };
    let x = (rect.x.max(0.0) as u32).min(original_rgba.width());
    let y = (rect.y.max(0.0) as u32).min(original_rgba.height());
    let width = (rect.width as u32).min(original_rgba.width().saturating_sub(x));
    let height = (rect.height as u32).min(original_rgba.height().saturating_sub(y));
    if width == 0 || height == 0 {
        return None;
    }
    use image::GenericImageView;
    Some(original_rgba.view(x, y, width, height).to_image())
}

/// 处理长截图非编辑态（就绪/录制/暂停）下的选区范围左键拦截与控制栏分发。
pub(super) fn handle_non_editing_press(
    state: &mut SnipperState,
    rect: Rect,
    pointer: Point,
    original_rgba: &RgbaImage,
) -> Option<bool> {
    let session = state.long_capture.as_mut()?;
    if session.is_editing {
        return None;
    }

    let in_bar = session
        .toolbar_rect
        .is_some_and(|r| r.contains(pointer.x, pointer.y));
    let in_sub = session
        .crop_submenu_rect
        .is_some_and(|r| r.contains(pointer.x, pointer.y));
    if in_bar || in_sub {
        return Some(handle_long_capture_press(state, pointer, original_rgba));
    }

    let closed_submenu = std::mem::take(&mut session.crop_sub_menu_open);
    if !state.can_adjust_selection() {
        return Some(closed_submenu);
    }
    if let Some(handle) = super::buttons::selection_handle_at(state, rect, pointer.x, pointer.y) {
        return Some(super::buttons::begin_selection_resize(state, handle));
    }
    if rect.contains(pointer.x, pointer.y) {
        return Some(super::buttons::begin_selection_move(
            state, rect, pointer.x, pointer.y,
        ));
    }
    Some(closed_submenu)
}
