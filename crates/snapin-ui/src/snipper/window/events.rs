//! 截图会话键盘事件响应与选区历史回溯。

use super::actions::{handle_action_copy, handle_action_pin, handle_action_save};
pub use super::state::{commit_active_text_input, SnipperState};
use super::types::SnipperOutcome;
use crate::snipper::selection::SelectionState;
use image::RgbaImage;
use snapin_platform::write_text_to_clipboard;
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

/// 处理键盘按键交互
pub fn handle_key_input(
    state: &mut SnipperState,
    key: &Key,
    key_state: ElementState,
    original_rgba: &RgbaImage,
) -> bool {
    if key_state != ElementState::Pressed {
        return false;
    }

    if state.active_text_input.is_some() {
        return handle_text_key_input(state, key);
    }

    match key {
        Key::Named(NamedKey::Escape) => handle_escape(state),
        Key::Named(NamedKey::Shift) => {
            state.magnifier.format = state.magnifier.format.next();
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") && !state.modifiers.control_key() => {
            handle_copy_hex_color(state, original_rgba);
            false
        }
        Key::Character(c) if c.eq_ignore_ascii_case("z") && state.modifiers.control_key() => {
            super::annotation_edit::clear(state);
            state.annotations.undo()
        }
        Key::Character(c) if c.eq_ignore_ascii_case("y") && state.modifiers.control_key() => {
            super::annotation_edit::clear(state);
            state.annotations.redo()
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") && state.modifiers.control_key() => {
            handle_action_copy(state, original_rgba);
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("s") && state.modifiers.control_key() => {
            handle_action_save(state, original_rgba);
            true
        }
        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::F3) => {
            handle_action_pin(state, original_rgba);
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("o") && !state.modifiers.control_key() => {
            if matches!(state.selection, SelectionState::Locked { .. }) {
                super::actions::handle_action_ocr(state, original_rgba);
                true
            } else {
                false
            }
        }
        Key::Character(c) if c.eq_ignore_ascii_case("t") && !state.modifiers.control_key() => {
            if matches!(state.selection, SelectionState::Locked { .. }) {
                super::actions::handle_action_translate(state, original_rgba);
                true
            } else {
                false
            }
        }
        Key::Character(c) if c == "," => handle_history_back(state, original_rgba),
        Key::Character(c) if c == "." => handle_history_forward(state, original_rgba),
        _ => false,
    }
}

/// 处理处于文本编辑中的按键交互
fn handle_text_key_input(state: &mut SnipperState, key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
            commit_active_text_input(state);
            true
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some(session) = &mut state.active_text_input {
                session.pop_char();
            }
            true
        }
        Key::Named(NamedKey::Space) => {
            if let Some(session) = &mut state.active_text_input {
                session.push_char(' ');
            }
            true
        }
        Key::Character(s) if !state.modifiers.control_key() => {
            if let Some(session) = &mut state.active_text_input {
                session.push_str(s.as_str());
            }
            true
        }
        _ => false,
    }
}

/// 逐级退出处理 (先提交文本 -> 再收起二级栏/工具 -> 再取消标注选中 -> 再取消选区 -> 再退出窗口)
pub fn handle_escape(state: &mut SnipperState) -> bool {
    if state.active_text_input.is_some() {
        commit_active_text_input(state);
        true
    } else if state.toolbar.active_tool.is_some() {
        state.toolbar.active_tool = None;
        state.toolbar.sub_rect = None;
        super::annotation_edit::clear(state);
        true
    } else if state.selected_annotation.is_some() {
        super::annotation_edit::clear(state);
        true
    } else if matches!(state.selection, SelectionState::Locked { .. }) {
        super::annotation_edit::clear(state);
        state.selection = SelectionState::Idle;
        true
    } else {
        state.outcome = Some(SnipperOutcome::Cancelled);
        true
    }
}

/// 复制放大镜中心取色结果
fn handle_copy_hex_color(state: &SnipperState, img: &RgbaImage) {
    let px = (state.cursor_pos.0 as i32).clamp(0, img.width().saturating_sub(1) as i32) as u32;
    let py = (state.cursor_pos.1 as i32).clamp(0, img.height().saturating_sub(1) as i32) as u32;
    let pixel = img.get_pixel(px, py);
    let hex = format!("#{:02X}{:02X}{:02X}", pixel[0], pixel[1], pixel[2]);
    let _ = write_text_to_clipboard(&hex);
}

/// 历史选区回溯向后
fn handle_history_back(state: &mut SnipperState, img: &RgbaImage) -> bool {
    if let Some(r) = state.history.previous_history() {
        super::annotation_edit::clear(state);
        state.selection = SelectionState::Locked {
            rect: r,
            active_drag: None,
        };
        state
            .toolbar
            .update_layout(&r, img.width() as f32, img.height() as f32);
        true
    } else {
        false
    }
}

/// 历史选区回溯向前
fn handle_history_forward(state: &mut SnipperState, img: &RgbaImage) -> bool {
    if let Some(r) = state.history.next_history() {
        super::annotation_edit::clear(state);
        state.selection = SelectionState::Locked {
            rect: r,
            active_drag: None,
        };
        state
            .toolbar
            .update_layout(&r, img.width() as f32, img.height() as f32);
        true
    } else {
        false
    }
}
