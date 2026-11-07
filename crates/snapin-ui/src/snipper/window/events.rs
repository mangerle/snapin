//! 截图会话键盘事件响应与选区历史回溯。

use super::actions::{handle_action_copy, handle_action_pin, handle_action_save};
use super::clipboard::SnipperClipboardAction;
pub use super::state::{commit_active_text_input, SnipperState};
use super::types::SnipperOutcome;
use crate::editor::toolbar::Tool;
use crate::snipper::selection::{SelectionConstraint, SelectionState};
use image::RgbaImage;
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

    if state.ocr.editor.is_some() {
        return handle_ocr_editor_key(state, key);
    }

    if state.ocr.menu.is_some() {
        return matches!(key, Key::Named(NamedKey::Escape)) && handle_escape(state);
    }

    if state.editor.annotation.active_text_input.is_some() {
        return handle_text_key_input(state, key);
    }

    if state.ocr.redaction_preview.is_some() && handle_redaction_preview_key(state, key) {
        return true;
    }

    match key {
        Key::Named(named) => handle_named_key(state, *named, original_rgba),
        Key::Character(character) => handle_character_key(state, character, original_rgba),
        _ => false,
    }
}

/// 处理命名按键，保持退出、窗口循环、选区微调与贴图动作的原有优先级。
fn handle_named_key(state: &mut SnipperState, key: NamedKey, original_rgba: &RgbaImage) -> bool {
    match key {
        NamedKey::Escape => handle_escape(state),
        NamedKey::Tab => cycle_window(state, !state.viewport.modifiers.shift_key()),
        NamedKey::ArrowLeft => nudge_locked_selection(state, -1, 0, original_rgba),
        NamedKey::ArrowRight => nudge_locked_selection(state, 1, 0, original_rgba),
        NamedKey::ArrowUp => nudge_locked_selection(state, 0, -1, original_rgba),
        NamedKey::ArrowDown => nudge_locked_selection(state, 0, 1, original_rgba),
        NamedKey::Shift => {
            state.viewport.magnifier.format = state.viewport.magnifier.format.next();
            true
        }
        NamedKey::Enter if state.viewport.modifiers.control_key() => handle_qr_open_link(state),
        NamedKey::Enter | NamedKey::F3 => handle_pin_or_finish_line(state, original_rgba),
        _ => false,
    }
}

/// Enter/F3 优先结束进行中的折线，否则将当前结果贴图为置顶窗口。
fn handle_pin_or_finish_line(state: &mut SnipperState, original_rgba: &RgbaImage) -> bool {
    if state.editor.annotation.toolbar.active_tool == Some(Tool::Line)
        && state.editor.annotation.stroke.points.len() >= 2
    {
        super::annotation_builder::commit_annotation(state);
    } else {
        handle_action_pin(state, original_rgba);
    }
    true
}

/// 处理字符快捷键，先匹配组合键，再匹配普通动作与选区约束。
fn handle_character_key(state: &mut SnipperState, key: &str, original_rgba: &RgbaImage) -> bool {
    if let Some(handled) = handle_control_character(state, key, original_rgba) {
        return handled;
    }
    if let Some(handled) = handle_action_character(state, key, original_rgba) {
        return handled;
    }
    handle_selection_character(state, key, original_rgba)
}

/// 处理依赖 Control 修饰键的字符命令。
fn handle_control_character(
    state: &mut SnipperState,
    key: &str,
    original_rgba: &RgbaImage,
) -> Option<bool> {
    if !state.viewport.modifiers.control_key() {
        return None;
    }
    match key.to_ascii_lowercase().as_str() {
        "z" => {
            let changed = state.editor.annotations.undo();
            // 撤销后新的文档最后一条接管可编辑权；文档为空则清除选中。
            super::annotation_edit::select_latest(state);
            Some(changed)
        }
        "y" => {
            let changed = state.editor.annotations.redo();
            super::annotation_edit::select_latest(state);
            Some(changed)
        }
        "c" => {
            handle_action_copy(state, original_rgba);
            Some(true)
        }
        "s" => {
            handle_action_save(state, original_rgba);
            Some(true)
        }
        _ => None,
    }
}

/// 处理不依赖 Control 的单键动作。
fn handle_action_character(
    state: &mut SnipperState,
    key: &str,
    original_rgba: &RgbaImage,
) -> Option<bool> {
    if key.eq_ignore_ascii_case("c") && !state.viewport.modifiers.control_key() {
        handle_copy_hex_color(state, original_rgba);
        return Some(false);
    }
    if !matches!(state.viewport.selection, SelectionState::Locked { .. }) {
        return None;
    }
    if key.eq_ignore_ascii_case("o") && state.viewport.modifiers.shift_key() {
        super::actions::handle_action_ocr_edit(state, original_rgba);
        return Some(true);
    }
    if state.viewport.modifiers.control_key() {
        return None;
    }
    match key.to_ascii_lowercase().as_str() {
        "o" => super::actions::handle_action_ocr(state, original_rgba),
        "t" => super::actions::handle_action_translate(state, original_rgba),
        "r" => super::actions::handle_action_redact(state, original_rgba),
        "q" => super::actions::handle_action_qr(state, original_rgba),
        _ => return None,
    }
    Some(true)
}

/// 处理历史回溯、固定尺寸约束等选区级快捷键。
fn handle_selection_character(
    state: &mut SnipperState,
    key: &str,
    original_rgba: &RgbaImage,
) -> bool {
    match key {
        "," => handle_history_back(state, original_rgba),
        "." => handle_history_forward(state, original_rgba),
        "1" => set_selection_constraint(state, SelectionConstraint::Free),
        "2" => set_selection_constraint(
            state,
            SelectionConstraint::Ratio {
                width: 1,
                height: 1,
            },
        ),
        "3" => set_selection_constraint(
            state,
            SelectionConstraint::Ratio {
                width: 4,
                height: 3,
            },
        ),
        "4" => set_selection_constraint(
            state,
            SelectionConstraint::Ratio {
                width: 16,
                height: 9,
            },
        ),
        "5" => set_selection_constraint(
            state,
            SelectionConstraint::Fixed {
                width: 800,
                height: 600,
            },
        ),
        "6" => set_selection_constraint(
            state,
            SelectionConstraint::Fixed {
                width: 1280,
                height: 720,
            },
        ),
        _ => false,
    }
}

/// 在窗口候选之间循环，Tab 向前、Shift+Tab 向后。
fn cycle_window(state: &mut SnipperState, forward: bool) -> bool {
    if !matches!(state.viewport.selection, SelectionState::Idle)
        || state.viewport.window_rects.is_empty()
    {
        return false;
    }
    let len = state.viewport.window_rects.len();
    let current =
        state
            .viewport
            .window_cycle
            .unwrap_or(if forward { len.saturating_sub(1) } else { 0 });
    let next = if forward {
        current.saturating_add(1) % len
    } else {
        current.saturating_add(len).saturating_sub(1) % len
    };
    state.viewport.window_cycle = Some(next);
    state.viewport.hovered_window = state.viewport.window_rects.get(next).copied();
    state.set_status_message(format!("窗口循环: {}/{}，短按左键吸附", next + 1, len));
    true
}

/// 设置选区尺寸约束；正在拖拽时立即按新约束刷新终点。
fn set_selection_constraint(state: &mut SnipperState, constraint: SelectionConstraint) -> bool {
    state.viewport.selection_constraint = constraint;
    if let SelectionState::Creating { start, current } = &mut state.viewport.selection {
        *current = constraint.constrain(*start, *current);
    }
    state.set_status_message(format!(
        "选区约束: {}；1 自由，2 1:1，3 4:3，4 16:9，5 800x600，6 1280x720",
        constraint.label()
    ));
    true
}

/// 锁定选区后使用方向键微调位置，Shift 加速为 10 像素。
fn nudge_locked_selection(
    state: &mut SnipperState,
    dx: i32,
    dy: i32,
    original_rgba: &RgbaImage,
) -> bool {
    let step = if state.viewport.modifiers.shift_key() {
        10
    } else {
        1
    };
    let SelectionState::Locked { rect, active_drag } = &mut state.viewport.selection else {
        return false;
    };
    if active_drag.is_some() {
        return false;
    }
    let max_x = (original_rgba.width() as f32 - rect.width).max(0.0);
    let max_y = (original_rgba.height() as f32 - rect.height).max(0.0);
    rect.x = (rect.x + (dx * step) as f32).clamp(0.0, max_x);
    rect.y = (rect.y + (dy * step) as f32).clamp(0.0, max_y);
    state.editor.annotation.toolbar.update_layout(
        rect,
        original_rgba.width() as f32,
        original_rgba.height() as f32,
    );
    let position = (rect.x as i32, rect.y as i32);
    state.set_status_message(format!("选区位置: ({}, {})", position.0, position.1));
    true
}

/// 处理智能打码交互阶段的退出键；其余快捷键继续交给全局处理。
fn handle_redaction_preview_key(state: &mut SnipperState, key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::Escape | NamedKey::Enter) => {
            state.ocr.redaction_preview = None;
            state.clear_status_message();
            true
        }
        _ => false,
    }
}

/// 处理 OCR 全文编辑框的键盘输入。
///
/// Enter 输入换行，Ctrl+Enter 将编辑后的全文写入剪贴板并在真实写入成功后关闭窗口，
/// Esc 只退出编辑框并保留当前截图会话。
fn handle_ocr_editor_key(state: &mut SnipperState, key: &Key) -> bool {
    let ctrl_pressed = state.viewport.modifiers.control_key();
    if matches!(key, Key::Named(NamedKey::Escape)) {
        state.ocr.editor = None;
        state.clear_status_message();
        return true;
    }
    if matches!(key, Key::Named(NamedKey::Enter)) && ctrl_pressed {
        let Some(text) = state.ocr.editor.as_ref().map(|editor| editor.finish_text()) else {
            return false;
        };
        if text.trim().is_empty() {
            state.set_status_message(snapin_foundation::i18n::tr("识别文字不能为空"));
        } else {
            state.submit_clipboard_text(text.clone(), SnipperClipboardAction::CopyText(text));
        }
        return true;
    }
    let Some(editor) = state.ocr.editor.as_mut() else {
        return false;
    };
    match key {
        Key::Named(NamedKey::Enter) => editor.push_str("\n"),
        Key::Named(NamedKey::Backspace) => editor.pop_char(),
        Key::Named(NamedKey::Space) => editor.push_str(" "),
        Key::Character(character) if !ctrl_pressed => editor.push_str(character),
        _ => {}
    }
    true
}

/// 打开最近二维码结果中的首个网页链接。
fn handle_qr_open_link(state: &mut SnipperState) -> bool {
    let Some(url) = state
        .ocr
        .qr_result
        .as_ref()
        .and_then(|codes| codes.first())
        .map(|code| code.text.clone())
    else {
        state.set_status_message(snapin_foundation::i18n::tr("请先识别二维码"));
        return true;
    };
    match state.session.desktop.open_url(&url) {
        Ok(()) => state.set_status_message(snapin_foundation::i18n::tr(
            "已请求系统浏览器打开二维码链接",
        )),
        Err(error) => {
            tracing::warn!("打开二维码链接失败: {error}");
            state.set_status_message(format!("打开二维码链接失败: {error}"));
        }
    }
    true
}

/// 处理处于文本编辑中的按键交互。
///
/// 具体字符增删与 Esc/Enter 语义由共享的 [`TextInputSession`] 处理，
/// 截图侧只负责把动作落到标注历史。
fn handle_text_key_input(state: &mut SnipperState, key: &Key) -> bool {
    let modifiers = crate::editor::text_input::KeyModifiers::new(
        state.viewport.modifiers.control_key(),
        state.viewport.modifiers.shift_key(),
        state.viewport.modifiers.alt_key(),
    );
    let action = match state.editor.annotation.active_text_input.as_mut() {
        Some(session) => crate::editor::text_input::handle_session_key(session, key, modifiers),
        None => return false,
    };
    match action {
        crate::editor::text_input::TextSessionAction::Handled => true,
        crate::editor::text_input::TextSessionAction::Commit => {
            commit_active_text_input(state);
            true
        }
        crate::editor::text_input::TextSessionAction::Cancel => {
            state.editor.annotation.active_text_input = None;
            true
        }
        crate::editor::text_input::TextSessionAction::Ignored => false,
    }
}

/// 逐级退出处理 (先提交文本 -> 再收起二级栏/工具 -> 再取消标注选中 -> 再取消选区 -> 再退出窗口)
pub fn handle_escape(state: &mut SnipperState) -> bool {
    if state.ocr.menu.is_some() {
        state.ocr.menu = None;
        true
    } else if state.ocr.editor.is_some() {
        state.ocr.editor = None;
        state.clear_status_message();
        true
    } else if state.ocr.redaction_preview.is_some() {
        // 马赛克在识别完成时已经写入标注历史，退出交互模式不得撤销用户当前选择。
        state.ocr.redaction_preview = None;
        state.clear_status_message();
        true
    } else if state.editor.annotation.active_text_input.is_some() {
        // Esc 统一取消当前输入，与贴图侧保持一致。
        state.editor.annotation.active_text_input = None;
        true
    } else if state.editor.annotation.toolbar.active_tool.is_some() {
        state.editor.annotation.toolbar.active_tool = None;
        state.editor.annotation.toolbar.sub_rect = None;
        super::annotation_edit::clear(state);
        true
    } else if state.editor.selected_annotation.is_some() {
        super::annotation_edit::clear(state);
        true
    } else if matches!(state.viewport.selection, SelectionState::Locked { .. }) {
        super::annotation_edit::clear(state);
        state.viewport.selection = SelectionState::Idle;
        true
    } else {
        state.session.outcome = Some(SnipperOutcome::Cancelled);
        true
    }
}

/// 复制放大镜中心取色结果
fn handle_copy_hex_color(state: &mut SnipperState, img: &RgbaImage) {
    let px =
        (state.viewport.cursor_pos.0 as i32).clamp(0, img.width().saturating_sub(1) as i32) as u32;
    let py =
        (state.viewport.cursor_pos.1 as i32).clamp(0, img.height().saturating_sub(1) as i32) as u32;
    let pixel = img.get_pixel(px, py);
    let hex = format!("#{:02X}{:02X}{:02X}", pixel[0], pixel[1], pixel[2]);
    state.submit_clipboard_text(
        hex.clone(),
        SnipperClipboardAction::TransientStatus {
            success_message: format!("已复制颜色值: {hex}"),
            failure_label: "复制颜色值",
        },
    );
}

/// 历史选区回溯向后
fn handle_history_back(state: &mut SnipperState, img: &RgbaImage) -> bool {
    if let Some(r) = state.viewport.history.previous_history() {
        super::annotation_edit::clear(state);
        state.viewport.selection = SelectionState::Locked {
            rect: r,
            active_drag: None,
        };
        state
            .editor
            .annotation
            .toolbar
            .update_layout(&r, img.width() as f32, img.height() as f32);
        true
    } else {
        false
    }
}

/// 历史选区回溯向前
fn handle_history_forward(state: &mut SnipperState, img: &RgbaImage) -> bool {
    if let Some(r) = state.viewport.history.next_history() {
        super::annotation_edit::clear(state);
        state.viewport.selection = SelectionState::Locked {
            rect: r,
            active_drag: None,
        };
        state
            .editor
            .annotation
            .toolbar
            .update_layout(&r, img.width() as f32, img.height() as f32);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snipper::window::ocr_editor::OcrTextEditorState;
    use winit::keyboard::ModifiersState;

    /// 编辑框内 Enter 必须换行，Ctrl+Enter 才提交复制。
    #[test]
    fn test_ocr_editor_enter_and_ctrl_enter() {
        let mut state = SnipperState::new();
        state.ocr.editor = Some(OcrTextEditorState::new("第一行".to_string()));
        let image = RgbaImage::new(100, 100);

        assert!(handle_key_input(
            &mut state,
            &Key::Named(NamedKey::Enter),
            ElementState::Pressed,
            &image,
        ));
        assert_eq!(
            state
                .ocr
                .editor
                .as_ref()
                .map(OcrTextEditorState::finish_text)
                .as_deref(),
            Some("第一行\n")
        );

        state.viewport.modifiers = ModifiersState::CONTROL;
        assert!(handle_key_input(
            &mut state,
            &Key::Named(NamedKey::Enter),
            ElementState::Pressed,
            &image,
        ));
        assert!(state.has_pending_clipboard());
    }
}
