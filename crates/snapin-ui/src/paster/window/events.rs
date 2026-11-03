//! 贴图窗口快捷键、鼠标拖拽、标注绘制、滚轮缩放与交互事件调度门面。

pub use super::state::PasterWindowState;

use super::annotation::{handle_toolbar_click, refresh_toolbar_layout};
use super::menu_actions::{
    copy_paster_and_close_to_clipboard, copy_paster_to_clipboard, dispatch_menu_action,
    save_paster_to_disk,
};
use super::state::apply_window_scale_resize;
use super::state::interaction::InteractionMode;
use crate::editor::toolbar::Tool;
use crate::paster::context_menu::{ContextMenuState, OpenMenuRequest};
use snapin_config::config::DoubleClickAction;
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

    // 文字会话激活时独占键盘：Esc 只取消会话，不得落到关闭贴图的分支
    if state.has_active_text_session() {
        return state.handle_text_session_key(key);
    }

    if state.interaction.context_menu.is_open {
        if let Key::Named(NamedKey::Escape) = key {
            state.interaction.context_menu.close();
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
        Key::Character(c) if c == "4" => state.rotate_ccw(window),
        Key::Character(c) if c.eq_ignore_ascii_case("h") => state.flip_horizontal(),
        Key::Character(c) if c.eq_ignore_ascii_case("v") => state.flip_vertical(),
        Key::Character(c) if c.eq_ignore_ascii_case("c") => {
            copy_paster_to_clipboard(state);
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
pub fn handle_paster_cursor_move(
    state: &mut PasterWindowState,
    pos: (f64, f64),
    window_size: (f32, f32),
) -> bool {
    state.viewport.cursor_pos = (pos.0 as f32, pos.1 as f32);

    if state.interaction.annotation.stroke.is_drawing {
        return state.update_stroke(state.viewport.cursor_pos);
    }

    let toolbar_hover_changed = if state.chrome.show_toolbar {
        refresh_toolbar_layout(state, window_size.0, window_size.1);
        state
            .interaction
            .annotation
            .toolbar
            .handle_mouse_move(state.viewport.cursor_pos.0, state.viewport.cursor_pos.1)
    } else {
        false
    };

    let menu_hover_changed = if state.interaction.context_menu.is_open {
        let items =
            ContextMenuState::build_main_items(state.chrome.show_toolbar, state.chrome.has_shadow);
        state.interaction.context_menu.handle_mouse_move(
            state.viewport.cursor_pos.into(),
            &items,
            state.chrome.smooth_scaling,
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
    if state.interaction.mode() == InteractionMode::Menu {
        state.interaction.context_menu.close();
    }

    let delta_y = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
    };

    if state.interaction.annotation.toolbar.handle_sub_wheel(
        state.viewport.cursor_pos.0,
        state.viewport.cursor_pos.1,
        delta_y,
    ) {
        if state.interaction.annotation.toolbar.focused_sub_control()
            == Some(crate::editor::toolbar::SubControl::CornerRadius)
        {
            state.document.annotations.adjust_last_corner_radius(
                state
                    .interaction
                    .annotation
                    .toolbar
                    .sub_options
                    .corner_radius,
            );
        }
        return true;
    }

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
    // 任何按下动作都意味着离开当前文字输入位置：先提交会话再分发
    // 工具栏切换、右键菜单或图像外拖拽，保证已输入内容不会丢失。
    let committed_text = button_state == ElementState::Pressed && state.commit_text_session();

    let handled = match (button, button_state) {
        (MouseButton::Right, ElementState::Pressed) => {
            // 右键打开菜单前取消未完成的一笔，避免拖拽预览残留在图像上
            state.interaction.annotation.stroke.points.clear();
            state.interaction.annotation.stroke.is_drawing = false;

            state
                .interaction
                .context_menu
                .set_ui_scale(state.viewport.ui_scale);
            state.interaction.context_menu.is_open = true;
            apply_window_scale_resize(state, window);

            let win_size = window.inner_size();
            let main_items = ContextMenuState::build_main_items(
                state.chrome.show_toolbar,
                state.chrome.has_shadow,
            );
            state.interaction.context_menu.open_at(OpenMenuRequest {
                x: state.viewport.cursor_pos.0,
                y: state.viewport.cursor_pos.1,
                window_width: win_size.width as f32,
                window_height: win_size.height as f32,
                main_items: &main_items,
            });
            true
        }
        (MouseButton::Left, ElementState::Pressed) => handle_left_press(state, window),
        (MouseButton::Left, ElementState::Released) => handle_left_release(state),
        _ => false,
    };

    // 即使只发生了文字提交（例如按下鼠标中键或点击图像外），也要重绘以清除输入框
    handled || committed_text
}

/// 左键按下：优先处理菜单与工具栏，其次在激活标注工具时开始一笔，最后才拖动窗口。
fn handle_left_press(state: &mut PasterWindowState, window: &Window) -> bool {
    if state.interaction.context_menu.is_open {
        return handle_context_menu_press(state, window);
    }

    let win_size = window.inner_size();
    if handle_toolbar_click(
        state,
        (win_size.width as f32, win_size.height as f32),
        window,
    ) {
        return true;
    }
    state.interaction.annotation.toolbar.clear_sub_focus();
    if handle_image_press(state) {
        return true;
    }
    handle_window_press(state, window)
}

/// 处理右键菜单点击及其关闭后的窗口尺寸重算。
fn handle_context_menu_press(state: &mut PasterWindowState, window: &Window) -> bool {
    let main_items =
        ContextMenuState::build_main_items(state.chrome.show_toolbar, state.chrome.has_shadow);
    let action = state.interaction.context_menu.handle_click(
        state.viewport.cursor_pos.into(),
        &main_items,
        state.chrome.smooth_scaling,
    );
    if let Some(action) = action {
        dispatch_menu_action(state, action, window);
    }
    if !state.interaction.context_menu.is_open {
        apply_window_scale_resize(state, window);
    }
    true
}

/// 文字工具开始 IME 会话，绘制工具开始一笔；图像外点击不消费。
fn handle_image_press(state: &mut PasterWindowState) -> bool {
    if !state.screen_point_inside_image(state.viewport.cursor_pos) {
        return false;
    }
    if state.interaction.annotation.toolbar.active_tool == Some(Tool::Text) {
        return state.begin_text_session(state.viewport.cursor_pos);
    }
    if state.interaction.annotation.toolbar.active_tool.is_some() {
        state.begin_stroke(state.viewport.cursor_pos);
        return true;
    }
    false
}

/// 处理双击动作，否则记录点击并开始拖动窗口。
fn handle_window_press(state: &mut PasterWindowState, window: &Window) -> bool {
    let now = Instant::now();
    if let Some(prev) = state.interaction.last_click_time {
        if now.duration_since(prev).as_millis() < 300 {
            state.interaction.last_click_time = None;
            apply_double_click_action(state);
            return true;
        }
    }
    state.interaction.last_click_time = Some(now);
    let _ = window.drag_window();
    false
}

/// 按配置执行双击行为。
fn apply_double_click_action(state: &mut PasterWindowState) {
    match state.services.config.paster.double_click_action {
        DoubleClickAction::CopyAndClose => {
            copy_paster_and_close_to_clipboard(state);
        }
        DoubleClickAction::Copy => {
            copy_paster_to_clipboard(state);
        }
        DoubleClickAction::Close => state.should_close = true,
    }
}

/// 左键松开：把当前笔迹提交进图像与标注历史，并清空预览状态。
fn handle_left_release(state: &mut PasterWindowState) -> bool {
    if !state.interaction.annotation.stroke.is_drawing {
        return false;
    }
    state.update_stroke(state.viewport.cursor_pos);
    state.commit_pending_stroke();
    // 即使空笔迹被丢弃也要重绘一次，确保画布上的拖拽预览被清掉
    true
}
