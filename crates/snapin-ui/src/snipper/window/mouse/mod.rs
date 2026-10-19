//! 截图交互鼠标移动、按键点击、滚轮缩放与选区拖拽处理。

pub mod toolbar_action;

use super::annotation_builder::commit_annotation;
use super::events::{commit_active_text_input, handle_escape, SnipperState};
use crate::snipper::selection::{DragMode, Rect, SelectionState};
use crate::snipper::toolbar::{StrokeWidthLevel, ToolbarItem};
use image::RgbaImage;
use toolbar_action::{dispatch_toolbar_action, handle_text_tool_click};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::window::Window;

/// 鼠标移动事件统一处理 (更新光标坐标、选区创建与调整手柄拖拽)
pub fn handle_cursor_move(
    state: &mut SnipperState,
    pos: (f64, f64),
    _window: &Window,
    screen_w: f32,
    screen_h: f32,
) -> bool {
    state.cursor_pos = pos;
    let (cx, cy) = (pos.0 as f32, pos.1 as f32);

    match &mut state.selection {
        SelectionState::Idle => true,
        SelectionState::Creating { current, .. } => {
            *current = (cx, cy);
            true
        }
        SelectionState::Locked { rect, active_drag } => {
            if let Some(drag) = active_drag {
                let (r, d) = super::drag_calculator::calculate_drag(rect, drag, (cx, cy));
                *rect = r;
                *active_drag = Some(d);
                state.toolbar.update_layout(rect, screen_w, screen_h);
                true
            } else if state.toolbar.active_tool.is_some() && !state.pending_stroke_points.is_empty()
            {
                let px = cx.clamp(rect.x, rect.x + rect.width);
                let py = cy.clamp(rect.y, rect.y + rect.height);
                match state.toolbar.active_tool {
                    Some(ToolbarItem::ToolPen) | Some(ToolbarItem::ToolMarker) => {
                        state.pending_stroke_points.push((px, py));
                    }
                    _ => {
                        if state.pending_stroke_points.len() > 1 {
                            state.pending_stroke_points[1] = (px, py);
                        } else {
                            state.pending_stroke_points.push((px, py));
                        }
                    }
                }
                true
            } else {
                state.toolbar.handle_mouse_move(cx, cy)
            }
        }
    }
}

/// 鼠标按键点击事件处理
pub fn handle_mouse_button(
    state: &mut SnipperState,
    button: MouseButton,
    button_state: ElementState,
    original_rgba: &RgbaImage,
) -> bool {
    let (cx, cy) = (state.cursor_pos.0 as f32, state.cursor_pos.1 as f32);

    if button == MouseButton::Right && button_state == ElementState::Pressed {
        return handle_escape(state);
    }

    if button != MouseButton::Left {
        return false;
    }

    match button_state {
        ElementState::Pressed => handle_left_press(state, cx, cy, original_rgba),
        ElementState::Released => handle_left_release(state, cx, cy, original_rgba),
    }
}

/// 鼠标左键按下交互
fn handle_left_press(
    state: &mut SnipperState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> bool {
    if let SelectionState::Locked { rect, .. } = state.selection {
        return handle_locked_press(state, rect, cx, cy, original_rgba);
    }

    if matches!(state.selection, SelectionState::Idle) {
        state.selection = SelectionState::Creating {
            start: (cx, cy),
            current: (cx, cy),
        };
        return true;
    }

    false
}

/// 处理选区已锁定状态下的左键按下分支
fn handle_locked_press(
    state: &mut SnipperState,
    rect: Rect,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> bool {
    // 优先检测是否点击在工具栏范围内，坚决拦截阻止事件穿透到画布
    if state.toolbar.contains_point(cx, cy) {
        commit_active_text_input(state);
        if let Some(item) = state.toolbar.handle_click(cx, cy) {
            dispatch_toolbar_action(state, item, original_rgba);
        }
        return true;
    }

    if state.toolbar.active_tool == Some(ToolbarItem::ToolText) {
        return handle_text_tool_click(state, cx, cy);
    }

    commit_active_text_input(state);

    // 未激活标注工具时，优先检测是否命中 8 个控制手柄中的任意一个进行微调缩放
    if state.toolbar.active_tool.is_none() {
        let handle_tolerance = 8.0 * state.toolbar.ui_scale;
        if let Some(handle) = rect.hit_test_handle(cx, cy, handle_tolerance) {
            if let SelectionState::Locked {
                ref mut active_drag,
                ..
            } = state.selection
            {
                *active_drag = Some(DragMode::Resize(handle));
            }
            return true;
        }
    }

    if rect.contains(cx, cy) {
        if state.toolbar.active_tool.is_some() {
            state.pending_stroke_points.clear();
            state.pending_stroke_points.push((cx, cy));
            return true;
        }
        if let SelectionState::Locked {
            ref mut active_drag,
            ..
        } = state.selection
        {
            *active_drag = Some(DragMode::Move {
                offset_x: (cx - rect.x) as i32,
                offset_y: (cy - rect.y) as i32,
            });
        }
        return true;
    }

    // 点击在已有选区外部：清除当前选区，允许重新框选
    state.toolbar.active_tool = None;
    state.pending_stroke_points.clear();
    state.selection = SelectionState::Creating {
        start: (cx, cy),
        current: (cx, cy),
    };
    true
}

/// 鼠标左键抬起交互
fn handle_left_release(
    state: &mut SnipperState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> bool {
    let screen_w = original_rgba.width() as f32;
    let screen_h = original_rgba.height() as f32;

    match &mut state.selection {
        SelectionState::Creating { start, current } => {
            let rect = Rect::from_points(*start, *current);
            if rect.width > 4.0 && rect.height > 4.0 {
                snapin_platform::save_last_selection((rect.x, rect.y, rect.width, rect.height));
                state.history.push(rect);
                state.toolbar.update_layout(&rect, screen_w, screen_h);
                state.selection = SelectionState::Locked {
                    rect,
                    active_drag: None,
                };
                super::events::activate_default_tool_if_configured(state);
                if state.mode == super::types::SnipMode::AutoCopy {
                    super::actions::handle_action_copy(state, original_rgba);
                }
            } else {
                state.selection = SelectionState::Idle;
            }
            true
        }
        SelectionState::Locked { rect, active_drag } => {
            // 若释放位置在工具栏区域内，清理暂存点并直接拦截，避免误提交标注
            if state.toolbar.contains_point(cx, cy) {
                state.pending_stroke_points.clear();
                return true;
            }

            if active_drag.is_some() {
                *active_drag = None;
                snapin_platform::save_last_selection((rect.x, rect.y, rect.width, rect.height));
                state.history.push(*rect);
                state.toolbar.update_layout(rect, screen_w, screen_h);
                true
            } else if state.toolbar.active_tool.is_some() && !state.pending_stroke_points.is_empty()
            {
                state.pending_stroke_points.push((cx, cy));
                commit_annotation(state);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// 处理截图会话中的鼠标滚轮事件 (连续动态调节当前标注与后续标注大小)
pub fn handle_mouse_wheel(state: &mut SnipperState, delta: MouseScrollDelta) -> bool {
    let delta_y = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
    };

    if delta_y.abs() < 0.1 {
        return false;
    }

    let is_text_mode = state.toolbar.active_tool == Some(ToolbarItem::ToolText)
        || state.active_text_input.is_some();

    if is_text_mode {
        let step = if delta_y > 0.0 { 2.0 } else { -2.0 };
        let new_font_size = (state.toolbar.sub_options.font_size + step).clamp(12.0, 72.0);
        state.toolbar.sub_options.font_size = new_font_size;

        if let Some(session) = &mut state.active_text_input {
            session.font_size = new_font_size;
        }

        state
            .annotations
            .adjust_last_action_size(state.toolbar.sub_options.custom_stroke_width, new_font_size);
        true
    } else {
        let step = if delta_y > 0.0 { 1.0 } else { -1.0 };
        let new_stroke_w = (state.toolbar.sub_options.custom_stroke_width + step).clamp(1.0, 48.0);
        state.toolbar.sub_options.custom_stroke_width = new_stroke_w;

        state.toolbar.sub_options.stroke_width = if new_stroke_w <= 2.5 {
            StrokeWidthLevel::Thin
        } else if new_stroke_w <= 5.5 {
            StrokeWidthLevel::Medium
        } else {
            StrokeWidthLevel::Thick
        };

        state
            .annotations
            .adjust_last_action_size(new_stroke_w, state.toolbar.sub_options.font_size);
        true
    }
}

#[cfg(test)]
mod tests;
