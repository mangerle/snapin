//! 录屏工具栏的鼠标悬停与点击路由。

use super::super::annotation_builder::commit_annotation;
use super::super::events::SnipperState;
use super::super::types::SnipperOutcome;
use super::PointerButtonEvent;
use crate::ports::RecordingRegion;
use crate::snipper::canvas::recording::{calculate_recording_layout, RecordingLayoutRequest};
use crate::snipper::selection::SelectionState;
use crate::snipper::window::state::{RecordingAction, RecordingItem};
use winit::event::{ElementState, MouseButton};

/// 处理录制工具栏悬停；返回是否需要重绘。
pub(crate) fn handle_recording_cursor_move(
    state: &mut SnipperState,
    position: (f32, f32),
    screen_size: (f32, f32),
) -> Option<bool> {
    if state
        .recording
        .as_mut()
        .is_some_and(|session| session.drag_toolbar_to(position))
    {
        return Some(true);
    }
    let selection = locked_rect(state)?;
    let scale = state.editor.annotation.toolbar.ui_scale;
    let layout = {
        let session = state.recording.as_ref()?;
        calculate_recording_layout(RecordingLayoutRequest {
            session,
            selection: &selection,
            screen_width: screen_size.0,
            screen_height: screen_size.1,
            scale,
        })
    };
    let hovered = if layout.contains(position.0, position.1) {
        layout.hit(position.0, position.1)
    } else {
        None
    };
    let changed = state
        .recording
        .as_mut()
        .is_some_and(|session| session.set_hovered(hovered));
    if layout.contains(position.0, position.1) {
        Some(changed)
    } else {
        None
    }
}

/// 处理录制工具栏点击；未命中工具栏时返回 `None`，让普通标注继续处理。
pub(crate) fn handle_recording_button(
    state: &mut SnipperState,
    event: PointerButtonEvent<'_>,
) -> Option<bool> {
    if event.button != MouseButton::Left {
        return None;
    }
    if event.button_state == ElementState::Released {
        let handled = state.recording.as_mut().is_some_and(|session| {
            let was_dragging = session.end_toolbar_drag();
            let press_consumed = session.take_mouse_press_consumed();
            was_dragging || press_consumed
        });
        return handled.then_some(true);
    }
    let selection = locked_rect(state)?;
    let (screen_width, screen_height) = (
        event.original_rgba.width() as f32,
        event.original_rgba.height() as f32,
    );
    let scale = state.editor.annotation.toolbar.ui_scale;
    let layout = {
        let session = state.recording.as_ref()?;
        calculate_recording_layout(RecordingLayoutRequest {
            session,
            selection: &selection,
            screen_width,
            screen_height,
            scale,
        })
    };
    if let Some(format) = layout.hit_menu(event.position.0, event.position.1) {
        if let Some(session) = state.recording.as_mut() {
            session.consume_mouse_press();
            session.set_format(format);
        }
        return Some(true);
    }
    let hit = layout.hit(event.position.0, event.position.1);
    tracing::info!(
        "录屏工具栏点击，坐标: ({:.1}, {:.1})，工具栏: ({:.1}, {:.1}) {:.1}x{:.1}，命中: {hit:?}",
        event.position.0,
        event.position.1,
        layout.bar_rect.x,
        layout.bar_rect.y,
        layout.bar_rect.width,
        layout.bar_rect.height,
    );
    if !layout.contains(event.position.0, event.position.1) {
        if state
            .recording
            .as_ref()
            .is_some_and(|session| session.settings_open())
        {
            if let Some(session) = state.recording.as_mut() {
                session.consume_mouse_press();
                session.toggle_settings();
            }
            return Some(true);
        }
        // 录屏栏通常位于选区外。录屏状态下点击选区外但未命中录屏栏时也必须
        // 消费事件，不能落到隐藏的普通截图工具栏并重新打开录屏会话。
        if !selection.contains(event.position.0, event.position.1) {
            if let Some(session) = state.recording.as_mut() {
                session.consume_mouse_press();
            }
            return Some(true);
        }
        return None;
    }
    if let Some(session) = state.recording.as_mut() {
        session.consume_mouse_press();
    }
    let Some(item) = hit else {
        return Some(true);
    };
    if item == RecordingItem::Action(RecordingAction::DragHandle) {
        if let Some(session) = state.recording.as_mut() {
            session.begin_toolbar_drag(event.position);
        }
        return Some(true);
    }
    match item {
        RecordingItem::Tool(tool) => {
            commit_annotation(state);
            state.editor.annotation.toolbar.active_tool = Some(tool);
        }
        RecordingItem::Action(action) => handle_action(state, action, event.original_rgba),
    }
    Some(true)
}

fn handle_action(
    state: &mut SnipperState,
    action: RecordingAction,
    original_rgba: &image::RgbaImage,
) {
    match action {
        RecordingAction::ToggleCapture => {
            let Some(region) = recording_region(state, original_rgba) else {
                state.set_status_message(snapin_foundation::i18n::tr("录制区域无效"));
                return;
            };
            let result = state
                .recording
                .as_mut()
                .map(|session| session.toggle_capture(region));
            match result {
                Some(Ok(())) => {
                    state.sync_recording_overlay();
                    match state.recording.as_ref().map(|session| session.status()) {
                        Some(super::super::state::RecordingStatus::Starting) => {
                            state.set_status_message(snapin_foundation::i18n::tr(
                                "正在启动屏幕录制",
                            ));
                        }
                        Some(super::super::state::RecordingStatus::Stopping) => {
                            state.set_status_message(snapin_foundation::i18n::tr("正在编码录屏"));
                        }
                        _ => state.clear_status_message(),
                    }
                }
                Some(Err(error)) => state.set_status_message(error.message),
                None => {}
            }
        }
        RecordingAction::TogglePause => {
            if let Some(session) = state.recording.as_mut() {
                if let Err(error) = session.toggle_pause() {
                    state.set_status_message(error.message);
                } else if session.status() == super::super::state::RecordingStatus::Paused {
                    state.set_status_message(snapin_foundation::i18n::tr("屏幕录制已暂停"));
                } else {
                    state.clear_status_message();
                }
            }
        }
        RecordingAction::ToggleAudio => {
            if let Some(session) = state.recording.as_mut() {
                session.toggle_audio();
            }
        }
        RecordingAction::ToggleCursor => {
            if let Some(session) = state.recording.as_mut() {
                session.toggle_cursor();
            }
        }
        RecordingAction::ToggleSettings => {
            if let Some(session) = state.recording.as_mut() {
                session.toggle_settings();
            }
        }
        RecordingAction::CycleFps => {
            if let Some(session) = state.recording.as_mut() {
                session.cycle_fps();
                let fps = session.fps();
                tracing::info!("录屏帧率已切换为 {fps}FPS");
                state.set_status_message(snapin_foundation::trf!(
                    "帧率已切换为 {}FPS",
                    "Frame rate switched to {} FPS",
                    fps
                ));
            }
        }
        RecordingAction::Undo => {
            if state.editor.annotations.undo() {
                super::super::annotation_edit::select_latest(state);
                state.sync_recording_overlay();
            }
        }
        RecordingAction::DragHandle => {}
        RecordingAction::Move => {
            state.set_status_message(snapin_foundation::i18n::tr("录制期间选区位置已锁定"));
        }
        RecordingAction::Close => {
            if let Some(mut session) = state.recording.take() {
                session.cancel();
            }
            state.session.outcome = Some(SnipperOutcome::Cancelled);
        }
        RecordingAction::Restart => {
            if let Some(session) = state.recording.as_mut() {
                session.request_restart();
            }
        }
    }
}

fn locked_rect(state: &SnipperState) -> Option<crate::geometry::Rect> {
    match state.viewport.selection {
        SelectionState::Locked { rect, .. } => Some(rect),
        _ => None,
    }
}

fn recording_region(
    state: &SnipperState,
    original_rgba: &image::RgbaImage,
) -> Option<RecordingRegion> {
    let selection = locked_rect(state)?;
    let (x, y, width, height) =
        selection.to_pixel_bounds(original_rgba.width(), original_rgba.height());
    let width = width & !1;
    let height = height & !1;
    (width >= 2 && height >= 2).then_some(RecordingRegion {
        x,
        y,
        width,
        height,
    })
}
