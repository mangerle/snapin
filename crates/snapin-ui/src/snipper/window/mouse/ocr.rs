//! 文字识别编辑框与右键菜单的鼠标事件处理。

use super::*;
use crate::editor::toolbar::{ToolbarAction, ToolbarItem};
use crate::snipper::window::clipboard::SnipperClipboardAction;
use crate::snipper::window::jobs::SnipperJobKind;
use crate::snipper::window::ocr_editor::{OcrEditorButton, ScrollbarHit};
use crate::snipper::window::ocr_menu::{OcrContextMenuState, OcrMenuAction};

/// 优先处理文字识别编辑框、右键菜单及其按钮入口；未命中时返回 `None`。
pub(super) fn handle_ocr_overlay_press(
    state: &mut SnipperState,
    button: MouseButton,
    button_state: ElementState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> Option<bool> {
    if state.ocr.editor.is_some() {
        handle_editor_mouse_press(state, button, button_state, cx, cy, original_rgba);
        return Some(true);
    }

    if state.ocr.menu.is_some() {
        handle_open_menu_press(state, button, button_state, cx, cy, original_rgba);
        return Some(true);
    }

    if button == MouseButton::Right
        && button_state == ElementState::Pressed
        && state.editor.annotation.toolbar.hit_test_main(cx, cy)
            == Some(ToolbarItem::Action(ToolbarAction::Ocr))
    {
        open_ocr_menu(state, original_rgba);
        return Some(true);
    }
    None
}

/// 处理已展开菜单的按下与释放；菜单项在释放时执行，避免释放事件穿透到画布。
fn handle_open_menu_press(
    state: &mut SnipperState,
    button: MouseButton,
    button_state: ElementState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) {
    if button == MouseButton::Left && button_state == ElementState::Released {
        let action = state
            .ocr
            .menu
            .as_ref()
            .and_then(|menu| menu.hit_test(cx, cy));
        state.ocr.menu = None;
        if let Some(action) = action {
            dispatch_ocr_menu_action(state, action, original_rgba);
        }
    } else if button == MouseButton::Right && button_state == ElementState::Pressed {
        state.ocr.menu = None;
    }
}

/// 以文字识别按钮为锚点打开右键菜单。
fn open_ocr_menu(state: &mut SnipperState, original_rgba: &RgbaImage) {
    let item = ToolbarItem::Action(ToolbarAction::Ocr);
    let Some(anchor) = state.editor.annotation.toolbar.main_item_rect(item) else {
        return;
    };
    state.ocr.menu = Some(OcrContextMenuState::open(
        anchor,
        original_rgba.width() as f32,
        original_rgba.height() as f32,
        state.editor.annotation.toolbar.ui_scale,
    ));
}

/// 派发文字识别右键菜单动作。
fn dispatch_ocr_menu_action(
    state: &mut SnipperState,
    action: OcrMenuAction,
    original_rgba: &RgbaImage,
) {
    match action {
        OcrMenuAction::CopyText => {
            crate::snipper::window::actions::handle_action_ocr(state, original_rgba)
        }
        OcrMenuAction::EditText => {
            crate::snipper::window::actions::handle_action_ocr_edit(state, original_rgba)
        }
    }
}

/// 处理文字识别全文编辑框的鼠标按下与释放。
fn handle_editor_mouse_press(
    state: &mut SnipperState,
    button: MouseButton,
    button_state: ElementState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) {
    if button != MouseButton::Left {
        return;
    }

    let screen_w = original_rgba.width() as f32;
    let screen_h = original_rgba.height() as f32;
    let scale = state.editor.annotation.toolbar.ui_scale;

    let Some(editor) = state.ocr.editor.as_mut() else {
        return;
    };
    editor.update_content_metrics(680.0 * scale, scale);
    let layout = editor.layout(screen_w, screen_h, scale);

    if button_state == ElementState::Pressed {
        if let Some(btn) = layout.hit_test_button(cx, cy) {
            editor.pressed_button = Some(btn);
            return;
        }

        if layout.hit_test_header_drag(cx, cy) {
            editor.drag_start = Some((cx - layout.panel.x, cy - layout.panel.y));
            return;
        }

        if let Some(hit) = layout.hit_test_scrollbar(cx, cy) {
            match hit {
                ScrollbarHit::Thumb => {
                    editor.is_dragging_scrollbar = true;
                    editor.scrollbar_drag_start_y = cy;
                    editor.scrollbar_drag_start_scroll_y = editor.scroll_y;
                }
                ScrollbarHit::Track => {
                    if let Some(thumb) = layout.scrollbar_thumb {
                        let step = layout.body.height * 0.8;
                        if cy < thumb.y {
                            editor.scroll_by(-step, layout.body.height);
                        } else {
                            editor.scroll_by(step, layout.body.height);
                        }
                    }
                }
            }
            return;
        }

        if layout.hit_test_body(cx, cy) {
            let padding_x = 12.0 * scale;
            let padding_y = 10.0 * scale;
            let max_text_width = (layout.body.width - padding_x * 2.0 - 12.0 * scale).max(10.0);
            let options = snapin_render::TextMeasureOptions::new(
                14.0 * scale,
                22.0 * scale,
                max_text_width,
                10000.0,
            );
            let hit_x = (cx - layout.body.x - padding_x).max(0.0);
            let hit_y = (cy - layout.body.y - padding_y + editor.scroll_y).max(0.0);
            let text = editor.display_text();
            let offset = snapin_render::with_thread_local_text_renderer(|renderer| {
                renderer.hit_test_text_offset(&text, hit_x, hit_y, options)
            });
            editor.cursor = offset;
            editor.selection_anchor = Some(offset);
            editor.is_selecting_text = true;
            editor.reset_blink();
        }
    } else {
        editor.drag_start = None;
        editor.is_selecting_text = false;
        editor.is_dragging_scrollbar = false;

        let pressed = editor.pressed_button.take();
        if let Some(btn) = pressed {
            if layout.hit_test_button(cx, cy) == Some(btn) {
                dispatch_editor_button(state, btn);
            }
        }
    }
}

/// 派发编辑框各实体按钮动作。
fn dispatch_editor_button(state: &mut SnipperState, btn: OcrEditorButton) {
    match btn {
        OcrEditorButton::CloseHeader | OcrEditorButton::Cancel => {
            state.ocr.editor = None;
            state.clear_status_message();
        }
        OcrEditorButton::CopyAndClose => {
            let Some(text) = state.ocr.editor.as_ref().map(|e| e.finish_text()) else {
                return;
            };
            if text.trim().is_empty() {
                state.set_status_message(snapin_foundation::i18n::tr("识别文字不能为空"));
            } else {
                state.submit_clipboard_text(text.clone(), SnipperClipboardAction::CopyText(text));
            }
        }
        OcrEditorButton::CopyOnly => {
            let text_to_copy = state
                .ocr
                .editor
                .as_ref()
                .and_then(|e| e.selected_text().or_else(|| Some(e.finish_text())));
            if let Some(text) = text_to_copy {
                if !text.is_empty() {
                    state.submit_clipboard_text(
                        text,
                        SnipperClipboardAction::TransientStatus {
                            success_message: snapin_foundation::i18n::tr("已复制文字到剪贴板")
                                .to_string(),
                            failure_label: "复制文字",
                        },
                    );
                }
            }
        }
        OcrEditorButton::SaveMarkdown => {
            let Some(text) = state.ocr.editor.as_ref().map(|e| e.finish_text()) else {
                return;
            };
            if text.trim().is_empty() {
                state.set_status_message(snapin_foundation::i18n::tr("识别文字不能为空"));
                return;
            }
            let lines = text.lines().map(ToString::to_string).collect::<Vec<_>>();
            crate::snipper::window::actions::dispatch_job(
                state,
                SnipperJobKind::Export,
                crate::ports::MediaJob::SaveOcrText {
                    lines,
                    format: crate::ports::OcrExportFormat::Markdown,
                },
            );
        }
        OcrEditorButton::Format => {
            if let Some(editor) = state.ocr.editor.as_mut() {
                editor.format_layout();
                state.set_status_message(snapin_foundation::i18n::tr("已整理文字排版"));
            }
        }
        OcrEditorButton::Clear => {
            if let Some(editor) = state.ocr.editor.as_mut() {
                editor.clear_text();
                state.set_status_message(snapin_foundation::i18n::tr("已清空识别文字"));
            }
        }
    }
}
