//! 鼠标点击与工具栏交互动作派发模块。

use super::super::actions::{handle_action_copy, handle_action_pin, handle_action_save};
use super::super::events::{commit_active_text_input, SnipperState};
use super::super::types::SnipperOutcome;
use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::ToolbarAction;
use crate::snipper::selection::SelectionState;
use image::RgbaImage;

/// 执行工具栏一次性动作；返回是否需要重绘。
///
/// `MouseThrough` 只属于贴图窗口工具栏，截图侧收到时按无操作处理。
pub fn dispatch_toolbar_action(
    state: &mut SnipperState,
    action: ToolbarAction,
    original_rgba: &RgbaImage,
) -> bool {
    match action {
        ToolbarAction::Undo => {
            let changed = state.editor.annotations.undo();
            // 历史变化后让新的文档最后一条接管可编辑权，避免选中停留在已被撤销的索引上。
            super::super::annotation_edit::select_latest(state);
            changed
        }
        ToolbarAction::MouseThrough => true,
        ToolbarAction::Redo => {
            let changed = state.editor.annotations.redo();
            super::super::annotation_edit::select_latest(state);
            changed
        }
        ToolbarAction::Copy => {
            handle_action_copy(state, original_rgba);
            true
        }
        ToolbarAction::Save => {
            handle_action_save(state, original_rgba);
            true
        }
        ToolbarAction::Pin => {
            handle_action_pin(state, original_rgba);
            true
        }
        ToolbarAction::Ocr => {
            super::super::actions::handle_action_ocr(state, original_rgba);
            true
        }
        ToolbarAction::SmartRedact => {
            super::super::actions::handle_action_redact(state, original_rgba);
            true
        }
        ToolbarAction::Qr => {
            super::super::actions::handle_action_qr(state, original_rgba);
            true
        }
        ToolbarAction::Translate => {
            super::super::actions::handle_action_translate(state, original_rgba);
            true
        }
        ToolbarAction::Close => {
            state.session.outcome = Some(SnipperOutcome::Cancelled);
            true
        }
        ToolbarAction::ClearAnnotations => {
            state.editor.annotations.clear();
            state.editor.selected_annotation = None;
            true
        }
    }
}

/// 处理文字工具点击选区开启输入
pub fn handle_text_tool_click(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    if let SelectionState::Locked { rect, .. } = &state.viewport.selection {
        if rect.contains(cx, cy) {
            commit_active_text_input(state);
            let c = state.editor.annotation.toolbar.sub_options.color;
            let rgba = (
                (c.red() * 255.0).round() as u8,
                (c.green() * 255.0).round() as u8,
                (c.blue() * 255.0).round() as u8,
                (c.alpha() * 255.0).round() as u8,
            );
            let mut session = TextInputSession::new(
                (cx, cy),
                rgba,
                state.editor.annotation.toolbar.sub_options.font_size,
            );
            session.apply_toolbar_options(&state.editor.annotation.toolbar.sub_options);
            state.editor.annotation.active_text_input = Some(session);
            return true;
        }
    }
    commit_active_text_input(state);
    false
}
