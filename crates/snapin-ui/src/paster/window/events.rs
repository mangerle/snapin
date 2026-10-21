//! 贴图窗口快捷键、鼠标拖拽、标注绘制、滚轮缩放与交互事件调度门面。

pub use super::state::PasterWindowState;

use super::annotation::{handle_toolbar_click, refresh_toolbar_layout};
use super::menu_actions::{copy_paster_to_clipboard, dispatch_menu_action, save_paster_to_disk};
use super::state::apply_window_scale_resize;
use crate::paster::context_menu::ContextMenuState;
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
            apply_window_scale_resize(state, window);
            return true;
        }
    }

    match key {
        Key::Named(NamedKey::Escape) => {
            state.should_close = true;
            true
        }
        Key::Character(c) if c == "1" => {
            state.adjust_opacity(-0.1);
            true
        }
        Key::Character(c) if c == "2" => {
            state.adjust_opacity(0.1);
            true
        }
        Key::Character(c) if c == "3" => state.rotate_cw(window),
        Key::Character(c) if c.eq_ignore_ascii_case("h") => state.flip_horizontal(),
        Key::Character(c) if c.eq_ignore_ascii_case("v") => state.flip_vertical(),
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

/// 处理鼠标移动事件：拖拽标注期间优先更新笔迹，否则处理工具栏悬停与右键菜单。
pub fn handle_paster_cursor_move(state: &mut PasterWindowState, pos: (f64, f64)) -> bool {
    state.cursor_pos = (pos.0 as f32, pos.1 as f32);

    if state.is_drawing {
        return state.update_stroke(state.cursor_pos);
    }

    let toolbar_hover_changed = if state.show_toolbar {
        refresh_toolbar_layout(state);
        state
            .toolbar
            .handle_mouse_move(state.cursor_pos.0, state.cursor_pos.1)
    } else {
        false
    };

    let menu_hover_changed = if state.context_menu.is_open {
        let items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
        state.context_menu.handle_mouse_move(
            state.cursor_pos.0,
            state.cursor_pos.1,
            &items,
            state.smooth_scaling,
        )
    } else {
        false
    };

    toolbar_hover_changed || menu_hover_changed
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
        state.zoom(delta_y, window);
        true
    } else {
        false
    }
}

/// 处理鼠标按键点击、标注拖拽与双击检测
pub fn handle_paster_mouse_button(
    state: &mut PasterWindowState,
    button: MouseButton,
    button_state: ElementState,
    window: &Window,
) -> bool {
    match (button, button_state) {
        (MouseButton::Right, ElementState::Pressed) => {
            // 右键打开菜单前取消未完成的一笔，避免拖拽预览残留在图像上
            state.pending_stroke_points.clear();
            state.is_drawing = false;

            state.context_menu.set_ui_scale(state.ui_scale);
            state.context_menu.is_open = true;
            apply_window_scale_resize(state, window);

            let win_size = window.inner_size();
            let main_items =
                ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
            state.context_menu.open_at(
                state.cursor_pos.0,
                state.cursor_pos.1,
                win_size.width as f32,
                win_size.height as f32,
                &main_items,
            );
            true
        }
        (MouseButton::Left, ElementState::Pressed) => handle_left_press(state, window),
        (MouseButton::Left, ElementState::Released) => handle_left_release(state),
        _ => false,
    }
}

/// 左键按下：优先处理菜单与工具栏，其次在激活标注工具时开始一笔，最后才拖动窗口。
fn handle_left_press(state: &mut PasterWindowState, window: &Window) -> bool {
    if state.context_menu.is_open {
        let main_items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
        let action = state.context_menu.handle_click(
            state.cursor_pos.0,
            state.cursor_pos.1,
            &main_items,
            state.smooth_scaling,
        );
        if let Some(act) = action {
            dispatch_menu_action(state, act, window);
        }
        if !state.context_menu.is_open {
            apply_window_scale_resize(state, window);
        }
        return true;
    }

    // 工具条命中优先于标注起点，避免点击按钮时误落笔
    if handle_toolbar_click(state) {
        return true;
    }

    // 有激活工具且按下点在贴图图像内：开始一笔，不触发窗口拖动与双击动作
    if state.toolbar.active_tool.is_some() && state.screen_point_inside_image(state.cursor_pos) {
        state.begin_stroke(state.cursor_pos);
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
    false
}

/// 左键松开：把当前笔迹提交进图像与标注历史，并清空预览状态。
fn handle_left_release(state: &mut PasterWindowState) -> bool {
    if !state.is_drawing {
        return false;
    }
    state.update_stroke(state.cursor_pos);
    state.commit_pending_stroke();
    // 即使空笔迹被丢弃也要重绘一次，确保画布上的拖拽预览被清掉
    true
}
