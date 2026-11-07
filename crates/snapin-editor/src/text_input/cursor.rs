//! 文本光标移动、字符边界安全计算、选区管理与按键调度。

use super::session::TextInputSession;
use winit::keyboard::{Key, NamedKey};

/// 文本输入会话处理中的键盘修饰键状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyModifiers {
    /// 构造包含 Ctrl、Shift 与 Alt 状态的修饰键集合。
    pub const fn new(ctrl: bool, shift: bool, alt: bool) -> Self {
        Self { ctrl, shift, alt }
    }
}

/// 文本会话按键处理结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSessionAction {
    /// 按键已被输入会话消费，内容或光标已更新
    Handled,
    /// 请求提交当前文本
    Commit,
    /// 请求取消当前文本
    Cancel,
    /// 请求复制选中文本或全部文本到剪贴板
    Copy(String),
    /// 请求剪切选中文本到剪贴板
    Cut(String),
    /// 请求从系统剪贴板粘贴文本
    PasteRequest,
    /// 与文本输入无关，交回调用方处理
    Ignored,
}

/// 获取光标前一个 UTF-8 字符的起始字节边界
pub(crate) fn prev_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let mut prev = cursor - 1;
    while prev > 0 && !text.is_char_boundary(prev) {
        prev -= 1;
    }
    prev
}

/// 获取光标后一个 UTF-8 字符的结束字节边界
pub(crate) fn next_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor >= text.len() {
        return text.len();
    }
    let mut next = cursor + 1;
    while next < text.len() && !text.is_char_boundary(next) {
        next += 1;
    }
    next
}

/// 查找光标所在行的起始字节偏移（行首）
pub(crate) fn find_line_home(text: &str, cursor: usize) -> usize {
    let safe_cursor = cursor.min(text.len());
    let prefix = &text[..safe_cursor];
    match prefix.rfind('\n') {
        Some(idx) => idx + 1,
        None => 0,
    }
}

/// 查找光标所在行的结束字节偏移（行尾，不包含换行符）
pub(crate) fn find_line_end(text: &str, cursor: usize) -> usize {
    let safe_cursor = cursor.min(text.len());
    let suffix = &text[safe_cursor..];
    match suffix.find('\n') {
        Some(idx) => safe_cursor + idx,
        None => text.len(),
    }
}

/// 统一处理文本输入会话中的按键事件。
pub fn handle_session_key(
    session: &mut TextInputSession,
    key: &Key,
    modifiers: KeyModifiers,
) -> TextSessionAction {
    if modifiers.ctrl && !modifiers.alt {
        return handle_ctrl_shortcuts(session, key);
    }

    match key {
        Key::Named(NamedKey::Escape) => TextSessionAction::Cancel,
        Key::Named(NamedKey::Enter) if modifiers.shift => {
            session.insert_char('\n');
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::Enter) => TextSessionAction::Commit,
        Key::Named(NamedKey::Backspace) => {
            session.delete_backward();
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::Delete) => {
            session.delete_forward();
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::ArrowLeft) => {
            session.move_cursor_left(modifiers.shift);
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::ArrowRight) => {
            session.move_cursor_right(modifiers.shift);
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::ArrowUp) => {
            session.move_cursor_up(modifiers.shift);
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::ArrowDown) => {
            session.move_cursor_down(modifiers.shift);
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::Home) => {
            session.move_cursor_home(modifiers.shift);
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::End) => {
            session.move_cursor_end(modifiers.shift);
            TextSessionAction::Handled
        }
        Key::Named(NamedKey::Space) => {
            session.insert_char(' ');
            TextSessionAction::Handled
        }
        Key::Character(text) if !text.is_empty() => {
            session.insert_str(text.as_str());
            TextSessionAction::Handled
        }
        _ => TextSessionAction::Ignored,
    }
}

/// 处理 Ctrl 组合快捷键（全选、复制、剪切、粘贴等）
fn handle_ctrl_shortcuts(session: &mut TextInputSession, key: &Key) -> TextSessionAction {
    match key {
        Key::Character(ch) if ch.eq_ignore_ascii_case("a") => {
            session.select_all();
            TextSessionAction::Handled
        }
        Key::Character(ch) if ch.eq_ignore_ascii_case("c") => {
            let copy_text = session
                .selected_text()
                .map(str::to_string)
                .unwrap_or_else(|| session.text.clone());
            if copy_text.is_empty() {
                TextSessionAction::Handled
            } else {
                TextSessionAction::Copy(copy_text)
            }
        }
        Key::Character(ch) if ch.eq_ignore_ascii_case("x") => {
            if let Some(selected) = session.selected_text().map(str::to_string) {
                session.delete_selection();
                TextSessionAction::Cut(selected)
            } else {
                TextSessionAction::Handled
            }
        }
        Key::Character(ch) if ch.eq_ignore_ascii_case("v") => TextSessionAction::PasteRequest,
        _ => TextSessionAction::Ignored,
    }
}
