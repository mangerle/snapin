//! 鼠标点击与工具栏交互动作派发模块。

use super::super::actions::{handle_action_copy, handle_action_pin, handle_action_save};
use super::super::events::{commit_active_text_input, SnipperState};
use super::super::text_input::TextInputSession;
use super::super::types::SnipperOutcome;
use crate::snipper::selection::SelectionState;
use crate::snipper::toolbar::ToolbarItem;
use image::RgbaImage;

/// 执行工具栏动作派发
pub fn dispatch_toolbar_action(
    state: &mut SnipperState,
    item: ToolbarItem,
    original_rgba: &RgbaImage,
) -> bool {
    match item {
        ToolbarItem::ActionUndo => state.annotations.undo(),
        ToolbarItem::ActionRedo => state.annotations.redo(),
        ToolbarItem::ActionCopy => {
            handle_action_copy(state, original_rgba);
            true
        }
        ToolbarItem::ActionSave => {
            handle_action_save(state, original_rgba);
            true
        }
        ToolbarItem::ActionPin => {
            handle_action_pin(state, original_rgba);
            true
        }
        ToolbarItem::ActionOcr => {
            super::super::actions::handle_action_ocr(state, original_rgba);
            true
        }
        ToolbarItem::ActionTranslate => {
            super::super::actions::handle_action_translate(state, original_rgba);
            true
        }
        ToolbarItem::ActionClose => {
            state.outcome = Some(SnipperOutcome::Cancelled);
            true
        }
        _ => true,
    }
}

/// 处理文字工具点击选区开启输入
pub fn handle_text_tool_click(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    if let SelectionState::Locked { rect, .. } = &state.selection {
        if rect.contains(cx, cy) {
            commit_active_text_input(state);
            let c = state.toolbar.sub_options.color;
            let rgba = (
                (c.red() * 255.0).round() as u8,
                (c.green() * 255.0).round() as u8,
                (c.blue() * 255.0).round() as u8,
                (c.alpha() * 255.0).round() as u8,
            );
            let font_sz = state.toolbar.sub_options.font_size;
            state.active_text_input = Some(TextInputSession::new((cx, cy), rgba, font_sz));
            return true;
        }
    }
    commit_active_text_input(state);
    false
}
