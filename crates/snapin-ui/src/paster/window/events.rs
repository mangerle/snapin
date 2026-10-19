//! 贴图窗口快捷键、鼠标拖拽、滚轮缩放与交互事件调度门面。

pub use super::state::PasterWindowState;
pub use super::toolbar_handler::check_and_handle_toolbar_click;

use super::menu_actions::{copy_paster_to_clipboard, dispatch_menu_action, save_paster_to_disk};
use super::state::apply_window_scale_resize;
use crate::paster::context_menu::ContextMenuState;
use crate::paster::transform::{flip_pixmap_horizontal, flip_pixmap_vertical, rotate_pixmap_cw};
use snapin_core::config::DoubleClickAction;
use std::time::Instant;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};
use winit::window::Window;

/// 处理键盘事件
pub fn handle_paster_key(
    state: &mut PasterWindowState,
    key: &Key,
    key_state: ElementState,
    window: &Window,
) -> bool {
    if key_state != ElementState::Pressed {
        return false;
    }

    if state.context_menu.is_open {
        if let Key::Named(NamedKey::Escape) = key {
            state.context_menu.close();
            return true;
        }
    }

    match key {
        Key::Named(NamedKey::Escape) => {
            state.should_close = true;
            true
        }
        Key::Character(c) if c == "1" => {
            state.item.transform.adjust_opacity(-0.1);
            true
        }
        Key::Character(c) if c == "2" => {
            state.item.transform.adjust_opacity(0.1);
            true
        }
        Key::Character(c) if c == "3" => {
            if let Some(rotated) = rotate_pixmap_cw(&state.item.pixmap) {
                state.item.pixmap = rotated;
                apply_window_scale_resize(state, window);
                true
            } else {
                false
            }
        }
        Key::Character(c) if c.eq_ignore_ascii_case("h") => {
            if let Some(flipped) = flip_pixmap_horizontal(&state.item.pixmap) {
                state.item.pixmap = flipped;
                true
            } else {
                false
            }
        }
        Key::Character(c) if c.eq_ignore_ascii_case("v") => {
            if let Some(flipped) = flip_pixmap_vertical(&state.item.pixmap) {
                state.item.pixmap = flipped;
                true
            } else {
                false
            }
        }
        Key::Character(c) if c.eq_ignore_ascii_case("c") => {
            copy_paster_to_clipboard(&state.item);
            false
        }
        Key::Character(c) if c.eq_ignore_ascii_case("s") => {
            save_paster_to_disk(state);
            false
        }
        _ => false,
    }
}

/// 处理鼠标移动事件
pub fn handle_paster_cursor_move(state: &mut PasterWindowState, pos: (f64, f64)) -> bool {
    state.cursor_pos = (pos.0 as f32, pos.1 as f32);
    if state.context_menu.is_open {
        let items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
        state.context_menu.handle_mouse_move(
            state.cursor_pos.0,
            state.cursor_pos.1,
            &items,
            state.smooth_scaling,
        )
    } else {
        false
    }
}

/// 处理鼠标滚轮缩放
pub fn handle_paster_scroll(
    state: &mut PasterWindowState,
    delta: MouseScrollDelta,
    window: &Window,
) -> bool {
    if state.context_menu.is_open {
        state.context_menu.close();
    }

    let delta_y = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
    };

    if delta_y.abs() > 0.01 {
        state.item.transform.zoom(delta_y);
        apply_window_scale_resize(state, window);
        true
    } else {
        false
    }
}

/// 处理鼠标按键点击与双击检测
pub fn handle_paster_mouse_button(
    state: &mut PasterWindowState,
    button: MouseButton,
    button_state: ElementState,
    window: &Window,
) -> bool {
    if button_state != ElementState::Pressed {
        return false;
    }

    if button == MouseButton::Right {
        state.context_menu.set_ui_scale(state.ui_scale);
        let win_size = window.inner_size();
        let main_items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
        state.context_menu.open_at(
            state.cursor_pos.0,
            state.cursor_pos.1,
            win_size.width as f32,
            win_size.height as f32,
            &main_items,
        );
        return true;
    }

    if button == MouseButton::Left {
        if state.context_menu.is_open {
            let main_items =
                ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
            let action = state.context_menu.handle_click(
                state.cursor_pos.0,
                state.cursor_pos.1,
                &main_items,
                state.smooth_scaling,
            );
            if let Some(act) = action {
                dispatch_menu_action(state, act, window);
            }
            return true;
        }

        // 优先拦截并响应对底部快捷工具条的点击，避免误触发底层窗口拖动
        if check_and_handle_toolbar_click(state, window) {
            return true;
        }

        let now = Instant::now();
        if let Some(prev) = state.last_click_time {
            if now.duration_since(prev).as_millis() < 300 {
                state.last_click_time = None;
                match state.config.paster.double_click_action {
                    DoubleClickAction::CopyAndClose => {
                        copy_paster_to_clipboard(&state.item);
                        state.should_close = true;
                    }
                    DoubleClickAction::Copy => {
                        copy_paster_to_clipboard(&state.item);
                    }
                    DoubleClickAction::Close => {
                        state.should_close = true;
                    }
                }
                return true;
            }
        }
        state.last_click_time = Some(now);
        let _ = window.drag_window();
    }

    false
}
