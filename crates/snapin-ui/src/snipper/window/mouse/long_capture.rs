//! 长截图控制条点击与选区首帧提取。

use super::super::types::SnipperOutcome;
use super::*;
use crate::geometry::Point;
use crate::snipper::window::state::{
    DirectionToggleOutcome, LongCaptureAction, LongCaptureSession,
};
use snapin_foundation::{i18n::tr, trf};
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

    dispatch_long_capture_action(state, action, pointer, original_rgba);
    true
}

fn dispatch_long_capture_action(
    state: &mut SnipperState,
    action: LongCaptureAction,
    pointer: Point,
    original_rgba: &RgbaImage,
) {
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
                state.set_status_message(tr("长截图已停止：可调整选区或裁剪，点击开始重新录制"));
            } else {
                start_long_capture_recording(state, original_rgba);
            }
        }
        LongCaptureAction::ToggleAutoScroll => {
            toggle_long_capture_auto_scroll(state, original_rgba)
        }
        LongCaptureAction::ToggleDirection => {
            if let Some(session) = state.long_capture.as_mut() {
                let message = match session.request_toggle_direction() {
                    DirectionToggleOutcome::Switched => tr("已切换长截图主方向并重置本轮拼接"),
                    DirectionToggleOutcome::NeedsConfirmation => {
                        tr("已有已拼接内容，再次点击方向按钮将清空重拍")
                    }
                };
                state.set_status_message(message);
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
                state.set_status_message(tr(msg));
            }
        }
        LongCaptureAction::CropHead => handle_crop_head_action(state),
        LongCaptureAction::CropTail => handle_crop_tail_action(state),
        LongCaptureAction::UndoCrop => {
            let undone = state
                .long_capture
                .as_mut()
                .is_some_and(LongCaptureSession::undo_crop);
            let message = if undone {
                tr("已撤销上一次裁剪")
            } else {
                tr("暂无可撤销的裁剪记录")
            };
            state.set_status_message(message);
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
    let cleared_confirm = std::mem::take(&mut session.pending_direction_confirm);
    if !state.can_adjust_selection() {
        return Some(closed_submenu || cleared_confirm);
    }
    if let Some(handle) = super::buttons::selection_handle_at(state, rect, pointer.x, pointer.y) {
        return Some(super::buttons::begin_selection_resize(state, handle));
    }
    if rect.contains(pointer.x, pointer.y) {
        return Some(super::buttons::begin_selection_move(
            state, rect, pointer.x, pointer.y,
        ));
    }
    Some(closed_submenu || cleared_confirm)
}

fn handle_crop_head_action(state: &mut SnipperState) {
    let Some(session) = state.long_capture.as_mut() else {
        return;
    };
    let direction = session.direction;
    let cropped = session.crop_head(30);
    if cropped == 0 {
        state.set_status_message(tr("已达最小尺寸，无法继续裁剪"));
        return;
    }
    let msg = match direction {
        StitchDirection::Vertical => trf!("已执行上裁剪 ({}px)", "Cropped top ({}px)", cropped),
        StitchDirection::Horizontal => trf!("已执行左裁剪 ({}px)", "Cropped left ({}px)", cropped),
    };
    state.set_status_message(msg);
}

fn handle_crop_tail_action(state: &mut SnipperState) {
    let Some(session) = state.long_capture.as_mut() else {
        return;
    };
    let was_capturing = session.is_capturing;
    let direction = session.direction;
    let cropped = session.crop_tail(30);
    if cropped == 0 {
        state.set_status_message(tr("已达最小尺寸，无法继续裁剪"));
        return;
    }
    if was_capturing && !session.is_capturing {
        state.set_status_message(tr(
            "已裁入首屏内部并暂停录制：可按 Ctrl+Z 撤销裁剪，或点击开始重录",
        ));
        return;
    }
    let msg = match direction {
        StitchDirection::Vertical => trf!("已执行下裁剪 ({}px)", "Cropped bottom ({}px)", cropped),
        StitchDirection::Horizontal => {
            trf!("已执行右裁剪 ({}px)", "Cropped right ({}px)", cropped)
        }
    };
    state.set_status_message(msg);
}

/// 若当前未处于录制状态，提取选区首帧并重启一轮长截图录制；返回是否处于有效录制态。
fn ensure_long_capture_recording(state: &mut SnipperState, original_rgba: &RgbaImage) -> bool {
    if state
        .long_capture
        .as_ref()
        .is_some_and(|session| session.is_capturing && !session.is_editing)
    {
        return true;
    }
    start_long_capture_recording(state, original_rgba)
}

fn start_long_capture_recording(state: &mut SnipperState, original_rgba: &RgbaImage) -> bool {
    let Some(first_frame) = current_selection_frame(state, original_rgba) else {
        state.set_status_message(tr("选区过小，无法开始长截图录制"));
        return false;
    };
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
    state.set_status_message(tr("长截图录制中：在选区内滚动鼠标自动拼接"));
    true
}

/// 切换长截图“自动滚到底部”开关，同步处理暂停态自动续拍。
pub(crate) fn toggle_long_capture_auto_scroll(state: &mut SnipperState, original_rgba: &RgbaImage) {
    let already_auto = state
        .long_capture
        .as_ref()
        .is_some_and(LongCaptureSession::is_auto_scrolling);
    if already_auto {
        if let Some(session) = state.long_capture.as_mut() {
            session.stop_auto_scroll();
        }
        state.set_status_message(tr("已停止自动滚动，请手动滚动或调整选区"));
        return;
    }

    if !ensure_long_capture_recording(state, original_rgba) {
        return;
    }
    if let Some(session) = state.long_capture.as_mut() {
        session.start_auto_scroll(std::time::Instant::now());
        state.set_status_message(tr(
            "自动滚到底录制中：再次点击自动滚动按钮或滚动鼠标可随时停止",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::snipper::selection::SelectionState;
    use image::{Rgba, RgbaImage};
    use snapin_render::stitching::StitchDirection;

    fn make_vertical_pattern(width: u32, height: u32, scroll_y: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            let wy = y.saturating_add(scroll_y);
            let r = ((wy.wrapping_mul(37).wrapping_add(x * 13)) % 251) as u8;
            let g = ((wy.wrapping_mul(73).wrapping_add(x * 29)) % 241) as u8;
            let b = ((((wy * wy) / 17).wrapping_add(x * 41)) % 239) as u8;
            Rgba([r, g, b, 255])
        })
    }

    fn handle_long_capture_action(
        state: &mut SnipperState,
        action: LongCaptureAction,
        bg: &RgbaImage,
    ) {
        dispatch_long_capture_action(state, action, Point::new(0.0, 0.0), bg);
    }

    #[test]
    fn test_toggle_direction_wipe_guard_and_confirm_reset() {
        let mut state = SnipperState::new();
        let rect = Rect::new(20.0, 20.0, 80.0, 120.0);
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        let frame0 = make_vertical_pattern(80, 120, 0);
        let mut session = LongCaptureSession::new(frame0);

        // 1. 尚无拼接增量时直接单次切换方向，无需二次确认
        state.long_capture = Some(session.clone());
        let bg = RgbaImage::new(200, 200);
        handle_long_capture_action(&mut state, LongCaptureAction::ToggleDirection, &bg);
        assert_eq!(
            state.long_capture.as_ref().expect("会话应存在").direction,
            StitchDirection::Horizontal
        );
        assert!(
            !state
                .long_capture
                .as_ref()
                .expect("会话应存在")
                .pending_direction_confirm
        );

        // 2. 存在已拼接增量时首次点击仅挂起确认，不得清空画布
        let frame1 = make_vertical_pattern(80, 120, 40);
        assert!(session.feed_frame_outcome(&frame1).is_stitched());
        assert_eq!(session.stitcher.dimensions(), (80, 160));
        state.long_capture = Some(session);

        handle_long_capture_action(&mut state, LongCaptureAction::ToggleDirection, &bg);
        let after_first = state.long_capture.as_ref().expect("会话应存在");
        assert_eq!(after_first.direction, StitchDirection::Vertical);
        assert_eq!(after_first.stitcher.dimensions(), (80, 160));
        assert!(after_first.pending_direction_confirm);
        assert!(state
            .session
            .status_message
            .as_deref()
            .is_some_and(|msg| msg.contains("再次点击方向按钮")));

        // 3. 插入其余交互动作（如点击裁剪菜单）自动撤销挂起确认
        handle_long_capture_action(&mut state, LongCaptureAction::ToggleCropSubMenu, &bg);
        assert!(
            !state
                .long_capture
                .as_ref()
                .expect("会话应存在")
                .pending_direction_confirm
        );

        // 4. 连续两次点击方向切换才正式清空画布切换方向
        handle_long_capture_action(&mut state, LongCaptureAction::ToggleDirection, &bg);
        assert!(
            state
                .long_capture
                .as_ref()
                .expect("会话应存在")
                .pending_direction_confirm
        );
        handle_long_capture_action(&mut state, LongCaptureAction::ToggleDirection, &bg);
        let switched = state.long_capture.as_ref().expect("会话应存在");
        assert_eq!(switched.direction, StitchDirection::Horizontal);
        assert_eq!(switched.stitcher.dimensions(), (80, 120));
        assert!(!switched.pending_direction_confirm);
    }

    #[test]
    fn test_crop_past_initial_pauses_capture_and_undo_restores() {
        let mut state = SnipperState::new();
        let rect = Rect::new(20.0, 20.0, 80.0, 120.0);
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        let frame0 = make_vertical_pattern(80, 120, 0);
        state.long_capture = Some(LongCaptureSession::new(frame0));
        let bg = RgbaImage::new(200, 200);

        // 录制未滚图时直接执行下裁剪 (30px)：突破 120px 降至 90px 并自动暂停防止断层拼接
        handle_long_capture_action(&mut state, LongCaptureAction::CropTail, &bg);
        let after_crop = state.long_capture.as_ref().expect("会话应存在");
        assert_eq!(after_crop.stitcher.dimensions(), (80, 90));
        assert!(!after_crop.is_capturing);
        assert!(state
            .session
            .status_message
            .as_deref()
            .is_some_and(|msg| msg.contains("已裁入首屏内部并暂停录制")));

        // 通过撤销裁剪恢复回 (80, 120) 及完整尾帧锚点
        handle_long_capture_action(&mut state, LongCaptureAction::UndoCrop, &bg);
        let restored = state.long_capture.as_ref().expect("会话应存在");
        assert_eq!(restored.stitcher.dimensions(), (80, 120));
        assert!(!restored.stitcher.is_tail_truncated_below_frame());
        assert_eq!(
            state.session.status_message.as_deref(),
            Some(tr("已撤销上一次裁剪"))
        );

        // 再次触发 UndoCrop 报告无记录
        handle_long_capture_action(&mut state, LongCaptureAction::UndoCrop, &bg);
        assert_eq!(
            state.session.status_message.as_deref(),
            Some(tr("暂无可撤销的裁剪记录"))
        );
    }

    #[test]
    fn test_toggle_auto_scroll_restarts_paused_session_and_stops_on_static_end() {
        let mut state = SnipperState::new();
        let rect = Rect::new(20.0, 20.0, 80.0, 120.0);
        state.viewport.selection = SelectionState::Locked {
            rect,
            active_drag: None,
        };
        let bg = make_vertical_pattern(200, 200, 0);
        let first = make_vertical_pattern(80, 120, 20);
        let mut session = LongCaptureSession::new(first);
        session.toggle_capture();
        assert!(!session.is_capturing);
        state.long_capture = Some(session);

        // 在暂停状态下直接点击“自动滚动”应自动恢复录制并开启自动滚动
        handle_long_capture_action(&mut state, LongCaptureAction::ToggleAutoScroll, &bg);
        let started = state.long_capture.as_ref().expect("会话应存在");
        assert!(started.is_capturing);
        assert!(started.is_auto_scrolling());

        // 投喂连续 3 帧静止相同画面后，应判定已滚动到底部并安全停止自动滚动
        for index in 0..3 {
            let req = state.jobs.begin_screen_capture().expect("应登记请求");
            let mut mutable_bg = bg.clone();
            state.apply_captured_long_frame(&mut mutable_bg, req, Ok(bg.clone()));
            let auto_now = state
                .long_capture
                .as_ref()
                .expect("会话应存在")
                .is_auto_scrolling();
            if index < 2 {
                assert!(auto_now, "第 {index} 帧静止尚不应提前停机");
            } else {
                assert!(!auto_now, "连续 3 帧静止后应自动停止滚动");
            }
        }
        assert_eq!(
            state.session.status_message.as_deref(),
            Some(tr("已自动滚动到底部并停止"))
        );
    }
}
