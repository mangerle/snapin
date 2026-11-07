//! 截图会话 OCR 全文编辑框键盘按键交互处理。

use crate::snipper::window::clipboard::SnipperClipboardAction;
use crate::snipper::window::state::SnipperState;
use winit::keyboard::{Key, NamedKey};

/// 处理 OCR 全文编辑框的键盘输入。
///
/// Enter 输入换行，Ctrl+Enter 将编辑后的全文写入剪贴板并在真实写入成功后关闭窗口，
/// Esc 只退出编辑框并保留当前截图会话。
pub(super) fn handle_ocr_editor_key(state: &mut SnipperState, key: &Key) -> bool {
    let ctrl_pressed = state.viewport.modifiers.control_key();
    let shift_pressed = state.viewport.modifiers.shift_key();

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

    let ui_scale = state.editor.annotation.toolbar.ui_scale;
    let Some(editor) = state.ocr.editor.as_mut() else {
        return false;
    };

    if ctrl_pressed && !state.viewport.modifiers.alt_key() {
        match key {
            Key::Character(c) if c.eq_ignore_ascii_case("a") => {
                editor.select_all();
                return true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                let text_to_copy = editor
                    .selected_text()
                    .unwrap_or_else(|| editor.finish_text());
                if !text_to_copy.is_empty() {
                    state.submit_clipboard_text(
                        text_to_copy,
                        SnipperClipboardAction::TransientStatus {
                            success_message: snapin_foundation::i18n::tr("已复制文字到剪贴板")
                                .to_string(),
                            failure_label: "复制文字",
                        },
                    );
                }
                return true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("x") => {
                if let Some(text_to_cut) = editor.selected_text() {
                    editor.delete_selection();
                    state.submit_clipboard_text(
                        text_to_cut,
                        SnipperClipboardAction::TransientStatus {
                            success_message: snapin_foundation::i18n::tr("已剪切文字到剪贴板")
                                .to_string(),
                            failure_label: "剪切文字",
                        },
                    );
                }
                return true;
            }
            Key::Character(c) if c.eq_ignore_ascii_case("v") => {
                state.request_clipboard_paste_into_ocr_editor();
                return true;
            }
            Key::Named(NamedKey::Home) => {
                editor.move_doc_start(shift_pressed);
                return true;
            }
            Key::Named(NamedKey::End) => {
                editor.move_doc_end(shift_pressed);
                return true;
            }
            _ => {}
        }
    }

    match key {
        Key::Named(NamedKey::ArrowLeft) => editor.move_left(shift_pressed),
        Key::Named(NamedKey::ArrowRight) => editor.move_right(shift_pressed),
        Key::Named(NamedKey::ArrowUp) => {
            editor.move_vertical(true, shift_pressed, 600.0 * ui_scale, ui_scale)
        }
        Key::Named(NamedKey::ArrowDown) => {
            editor.move_vertical(false, shift_pressed, 600.0 * ui_scale, ui_scale)
        }
        Key::Named(NamedKey::Home) => editor.move_home(shift_pressed),
        Key::Named(NamedKey::End) => editor.move_end(shift_pressed),
        Key::Named(NamedKey::Delete) => editor.delete_forward(),
        Key::Named(NamedKey::Backspace) => editor.pop_char_backward(),
        Key::Named(NamedKey::Enter) => editor.insert_char('\n'),
        Key::Named(NamedKey::Space) => editor.insert_char(' '),
        Key::Character(character) if !ctrl_pressed => editor.insert_str(character),
        _ => return false,
    }
    true
}
