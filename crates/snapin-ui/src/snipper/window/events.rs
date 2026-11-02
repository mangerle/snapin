//! 截图会话键盘事件响应与选区历史回溯。

use super::actions::{handle_action_copy, handle_action_pin, handle_action_save};
pub use super::state::{commit_active_text_input, SnipperState};
use super::types::SnipperOutcome;
use crate::editor::text_input::TextInputSession;
use crate::ports::OcrExportFormat;
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

    if state.active_text_input.is_some() {
        return handle_text_key_input(state, key);
    }

    if state.redaction_preview.is_some() && handle_redaction_preview_key(state, key) {
        return true;
    }

    if state.ocr_panel.is_some() {
        return handle_ocr_panel_key(state, key, original_rgba);
    }

    match key {
        Key::Named(NamedKey::Escape) => handle_escape(state),
        Key::Named(NamedKey::Tab) => cycle_window(state, !state.modifiers.shift_key()),
        Key::Named(NamedKey::ArrowLeft) => nudge_locked_selection(state, -1, 0, original_rgba),
        Key::Named(NamedKey::ArrowRight) => nudge_locked_selection(state, 1, 0, original_rgba),
        Key::Named(NamedKey::ArrowUp) => nudge_locked_selection(state, 0, -1, original_rgba),
        Key::Named(NamedKey::ArrowDown) => nudge_locked_selection(state, 0, 1, original_rgba),
        Key::Named(NamedKey::Shift) => {
            state.magnifier.format = state.magnifier.format.next();
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") && !state.modifiers.control_key() => {
            handle_copy_hex_color(state, original_rgba);
            false
        }
        Key::Character(c) if c.eq_ignore_ascii_case("z") && state.modifiers.control_key() => {
            let changed = state.annotations.undo();
            // 撤销后新的文档最后一条接管可编辑权；文档为空则清除选中。
            super::annotation_edit::select_latest(state);
            changed
        }
        Key::Character(c) if c.eq_ignore_ascii_case("y") && state.modifiers.control_key() => {
            let changed = state.annotations.redo();
            super::annotation_edit::select_latest(state);
            changed
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") && state.modifiers.control_key() => {
            handle_action_copy(state, original_rgba);
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("s") && state.modifiers.control_key() => {
            handle_action_save(state, original_rgba);
            true
        }
        Key::Named(NamedKey::Enter) if state.modifiers.control_key() => handle_qr_open_link(state),
        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::F3) => {
            if state.toolbar.active_tool == Some(crate::editor::toolbar::Tool::Line)
                && state.stroke.points.len() >= 2
            {
                super::annotation_builder::commit_annotation(state);
                return true;
            }
            handle_action_pin(state, original_rgba);
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("o") && state.modifiers.shift_key() => {
            if matches!(state.selection, SelectionState::Locked { .. }) {
                super::actions::handle_action_ocr_panel(state, original_rgba);
                true
            } else {
                false
            }
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
        Key::Character(c) if c.eq_ignore_ascii_case("r") && !state.modifiers.control_key() => {
            if matches!(state.selection, SelectionState::Locked { .. }) {
                super::actions::handle_action_redact(state, original_rgba);
                true
            } else {
                false
            }
        }
        Key::Character(c) if c.eq_ignore_ascii_case("q") && !state.modifiers.control_key() => {
            if matches!(state.selection, SelectionState::Locked { .. }) {
                super::actions::handle_action_qr(state, original_rgba);
                true
            } else {
                false
            }
        }
        Key::Character(c) if c == "," => handle_history_back(state, original_rgba),
        Key::Character(c) if c == "." => handle_history_forward(state, original_rgba),
        Key::Character(c) if c == "1" => set_selection_constraint(state, SelectionConstraint::Free),
        Key::Character(c) if c == "2" => set_selection_constraint(
            state,
            SelectionConstraint::Ratio {
                width: 1,
                height: 1,
            },
        ),
        Key::Character(c) if c == "3" => set_selection_constraint(
            state,
            SelectionConstraint::Ratio {
                width: 4,
                height: 3,
            },
        ),
        Key::Character(c) if c == "4" => set_selection_constraint(
            state,
            SelectionConstraint::Ratio {
                width: 16,
                height: 9,
            },
        ),
        Key::Character(c) if c == "5" => set_selection_constraint(
            state,
            SelectionConstraint::Fixed {
                width: 800,
                height: 600,
            },
        ),
        Key::Character(c) if c == "6" => set_selection_constraint(
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
    if !matches!(state.selection, SelectionState::Idle) || state.window_rects.is_empty() {
        return false;
    }
    let len = state.window_rects.len();
    let current = state
        .window_cycle
        .unwrap_or(if forward { len.saturating_sub(1) } else { 0 });
    let next = if forward {
        current.saturating_add(1) % len
    } else {
        current.saturating_add(len).saturating_sub(1) % len
    };
    state.window_cycle = Some(next);
    state.hovered_window = state.window_rects.get(next).copied();
    state.set_status_message(format!("窗口循环: {}/{}，短按左键吸附", next + 1, len));
    true
}

/// 设置选区尺寸约束；正在拖拽时立即按新约束刷新终点。
fn set_selection_constraint(state: &mut SnipperState, constraint: SelectionConstraint) -> bool {
    state.selection_constraint = constraint;
    if let SelectionState::Creating { start, current } = &mut state.selection {
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
    let step = if state.modifiers.shift_key() { 10 } else { 1 };
    let SelectionState::Locked { rect, active_drag } = &mut state.selection else {
        return false;
    };
    if active_drag.is_some() {
        return false;
    }
    let max_x = (original_rgba.width() as f32 - rect.width).max(0.0);
    let max_y = (original_rgba.height() as f32 - rect.height).max(0.0);
    rect.x = (rect.x + (dx * step) as f32).clamp(0.0, max_x);
    rect.y = (rect.y + (dy * step) as f32).clamp(0.0, max_y);
    state.toolbar.update_layout(
        rect,
        original_rgba.width() as f32,
        original_rgba.height() as f32,
    );
    let position = (rect.x as i32, rect.y as i32);
    state.set_status_message(format!("选区位置: ({}, {})", position.0, position.1));
    true
}

/// 处理 OCR 结果面板键盘操作。
fn handle_ocr_panel_key(state: &mut SnipperState, key: &Key, original_rgba: &RgbaImage) -> bool {
    let scale = state.toolbar.ui_scale;
    let screen_width = original_rgba.width() as f32;
    let screen_height = original_rgba.height() as f32;
    match key {
        Key::Named(NamedKey::Escape) => {
            state.ocr_panel = None;
            state.ocr_edit_row = None;
            state.active_text_input = None;
            state.set_status_message(snapin_core::i18n::tr("已关闭 OCR 结果面板"));
            true
        }
        Key::Named(NamedKey::ArrowUp) => {
            let visible = state
                .ocr_panel
                .as_ref()
                .map(|panel| panel.visible_rows(screen_height, scale))
                .unwrap_or(1);
            if let Some(panel) = state.ocr_panel.as_mut() {
                panel.move_cursor(-1, visible);
            }
            true
        }
        Key::Named(NamedKey::ArrowDown) => {
            let visible = state
                .ocr_panel
                .as_ref()
                .map(|panel| panel.visible_rows(screen_height, scale))
                .unwrap_or(1);
            if let Some(panel) = state.ocr_panel.as_mut() {
                panel.move_cursor(1, visible);
            }
            true
        }
        Key::Named(NamedKey::Space) => {
            if let Some(panel) = state.ocr_panel.as_mut() {
                panel.toggle_cursor();
            }
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("a") => {
            if let Some(panel) = state.ocr_panel.as_mut() {
                panel.toggle_all();
            }
            true
        }
        Key::Named(NamedKey::Enter) => {
            begin_ocr_row_edit(state, screen_width, screen_height, scale);
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("m") => {
            super::actions::handle_action_export_ocr(state, OcrExportFormat::Markdown);
            true
        }
        Key::Character(c) if c.eq_ignore_ascii_case("j") => {
            super::actions::handle_action_export_ocr(state, OcrExportFormat::Json);
            true
        }
        _ => true,
    }
}

/// 为当前 OCR 行创建文本编辑会话，提交时回写该行。
fn begin_ocr_row_edit(state: &mut SnipperState, screen_width: f32, screen_height: f32, scale: f32) {
    let Some(panel) = state.ocr_panel.as_ref() else {
        return;
    };
    let Some(line) = panel.lines.get(panel.cursor) else {
        return;
    };
    let Some(pos) = panel.editor_pos(screen_width, screen_height, scale) else {
        return;
    };
    let mut session = TextInputSession::new(pos, (255, 255, 255, 255), 14.0);
    session.text = line.text.clone();
    state.ocr_edit_row = Some(panel.cursor);
    state.active_text_input = Some(session);
    state.set_status_message(snapin_core::i18n::tr("编辑 OCR 文本；Enter 确认，Esc 取消"));
}

/// 处理智能打码交互阶段的退出键；其余快捷键继续交给全局处理。
fn handle_redaction_preview_key(state: &mut SnipperState, key: &Key) -> bool {
    match key {
        Key::Named(NamedKey::Escape | NamedKey::Enter) => {
            state.redaction_preview = None;
            state.clear_status_message();
            true
        }
        _ => false,
    }
}

/// 打开最近二维码结果中的首个网页链接。
fn handle_qr_open_link(state: &mut SnipperState) -> bool {
    let Some(url) = state
        .qr_result
        .as_ref()
        .and_then(|codes| codes.first())
        .map(|code| code.text.clone())
    else {
        state.set_status_message(snapin_core::i18n::tr("请先识别二维码"));
        return true;
    };
    match state.desktop.open_url(&url) {
        Ok(()) => state.set_status_message(snapin_core::i18n::tr("已请求系统浏览器打开二维码链接")),
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
    let ctrl_pressed = state.modifiers.control_key();
    let action = match state.active_text_input.as_mut() {
        Some(session) => crate::editor::text_input::handle_session_key(session, key, ctrl_pressed),
        None => return false,
    };
    match action {
        crate::editor::text_input::TextSessionAction::Handled => true,
        crate::editor::text_input::TextSessionAction::Commit => {
            commit_active_text_input(state);
            true
        }
        crate::editor::text_input::TextSessionAction::Cancel => {
            state.active_text_input = None;
            state.ocr_edit_row = None;
            true
        }
        crate::editor::text_input::TextSessionAction::Ignored => false,
    }
}

/// 逐级退出处理 (先提交文本 -> 再收起二级栏/工具 -> 再取消标注选中 -> 再取消选区 -> 再退出窗口)
pub fn handle_escape(state: &mut SnipperState) -> bool {
    if state.ocr_panel.is_some() {
        state.ocr_panel = None;
        state.ocr_edit_row = None;
        state.active_text_input = None;
        state.set_status_message(snapin_core::i18n::tr("已关闭 OCR 结果面板"));
        true
    } else if state.redaction_preview.is_some() {
        // 马赛克在识别完成时已经写入标注历史，退出交互模式不得撤销用户当前选择。
        state.redaction_preview = None;
        state.clear_status_message();
        true
    } else if state.active_text_input.is_some() {
        // Esc 统一取消当前输入，与贴图侧保持一致。
        state.active_text_input = None;
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
    if let Err(error) = state.desktop.write_text_clipboard(&hex) {
        tracing::warn!("复制取色结果到系统剪贴板失败: {error}");
    }
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
