//! 文字识别编辑框与右键菜单的鼠标事件处理。

use super::*;
use crate::editor::toolbar::{ToolbarAction, ToolbarItem};
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
