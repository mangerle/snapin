//! 截图交互鼠标移动、按键点击、滚轮缩放与选区拖拽处理。

use super::actions::{handle_action_copy, handle_action_pin, handle_action_save};
use super::annotation_builder::commit_annotation;
use super::events::{commit_active_text_input, handle_escape, SnipperState};
use super::text_input::TextInputSession;
use super::types::SnipperOutcome;
use crate::snipper::selection::{DragMode, Rect, SelectionState};
use crate::snipper::toolbar::{StrokeWidthLevel, ToolbarItem};
use image::RgbaImage;
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
                let p = (cx, cy);
                let (r, d) = match drag {
                    DragMode::Move { offset_x, offset_y } => (
                        Rect {
                            x: p.0 - *offset_x as f32,
                            y: p.1 - *offset_y as f32,
                            ..*rect
                        },
                        DragMode::Move {
                            offset_x: *offset_x,
                            offset_y: *offset_y,
                        },
                    ),
                    DragMode::Resize(h) => {
                        (Rect::from_points((rect.x, rect.y), p), DragMode::Resize(*h))
                    }
                };
                *rect = r;
                *active_drag = Some(d);
                state.toolbar.update_layout(rect, screen_w, screen_h);
                true
            } else if state.toolbar.active_tool.is_some() && !state.pending_stroke_points.is_empty()
            {
                match state.toolbar.active_tool {
                    Some(ToolbarItem::ToolPen) | Some(ToolbarItem::ToolMarker) => {
                        state.pending_stroke_points.push((cx, cy));
                    }
                    _ => {
                        if state.pending_stroke_points.len() > 1 {
                            state.pending_stroke_points[1] = (cx, cy);
                        } else {
                            state.pending_stroke_points.push((cx, cy));
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
    if matches!(state.selection, SelectionState::Locked { .. }) {
        if let Some(item) = state.toolbar.handle_click(cx, cy) {
            commit_active_text_input(state);
            return dispatch_toolbar_action(state, item, original_rgba);
        }

        if state.toolbar.active_tool == Some(ToolbarItem::ToolText) {
            return handle_text_tool_click(state, cx, cy);
        }

        commit_active_text_input(state);

        if state.toolbar.active_tool.is_some() {
            state.pending_stroke_points.clear();
            state.pending_stroke_points.push((cx, cy));
            return true;
        }
    }

    match state.selection {
        SelectionState::Idle => {
            state.selection = SelectionState::Creating {
                start: (cx, cy),
                current: (cx, cy),
            };
            true
        }
        SelectionState::Locked {
            rect,
            ref mut active_drag,
        } if rect.contains(cx, cy) => {
            *active_drag = Some(DragMode::Move {
                offset_x: (cx - rect.x) as i32,
                offset_y: (cy - rect.y) as i32,
            });
            true
        }
        _ => false,
    }
}

/// 处理文字工具点击选区开启输入
fn handle_text_tool_click(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    if let SelectionState::Locked { rect, .. } = &state.selection {
        if rect.contains(cx, cy) {
            commit_active_text_input(state);
            let c = state.toolbar.sub_options.color;
            let rgba = (
                (c.red() * 255.0).round() as u8,
                (c.green() * 255.0).round() as u8,
                (c.blue() * 255.0).round() as u8,
                (c.alpha() * 255.0).round() as u8,
            );
            let font_sz = state.toolbar.sub_options.font_size;
            state.active_text_input = Some(TextInputSession::new((cx, cy), rgba, font_sz));
            return true;
        }
    }
    commit_active_text_input(state);
    false
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
                state.history.push(rect);
                state.toolbar.update_layout(&rect, screen_w, screen_h);
                state.selection = SelectionState::Locked {
                    rect,
                    active_drag: None,
                };
            } else {
                state.selection = SelectionState::Idle;
            }
            true
        }
        SelectionState::Locked { rect, active_drag } => {
            if active_drag.is_some() {
                *active_drag = None;
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

/// 执行工具栏动作派发
fn dispatch_toolbar_action(
    state: &mut SnipperState,
    item: ToolbarItem,
    original_rgba: &RgbaImage,
) -> bool {
    match item {
        ToolbarItem::ActionUndo => {
            state.next_badge_number = state.next_badge_number.saturating_sub(1).max(1);
            state.annotations.undo()
        }
        ToolbarItem::ActionRedo => state.annotations.redo(),
        ToolbarItem::ActionCopy => {
            handle_action_copy(state, original_rgba);
            true
        }
        ToolbarItem::ActionSave => {
            handle_action_save(state, original_rgba);
            true
        }
        ToolbarItem::ActionPin => {
            handle_action_pin(state, original_rgba);
            true
        }
        ToolbarItem::ActionClose => {
            state.outcome = Some(SnipperOutcome::Cancelled);
            true
        }
        _ => true,
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
mod tests {
    use super::*;
    use crate::annotation::shapes::RectAction;

    #[test]
    fn test_mouse_wheel_continuous_stroke_width_and_retroactive_adjust() {
        let mut state = SnipperState::new();
        // 初始粗细为 4.0
        assert_eq!(state.toolbar.sub_options.custom_stroke_width, 4.0);

        // 模拟已绘制一个矩形标注
        state.annotations.push(Box::new(RectAction {
            rect: (10.0, 10.0, 100.0, 100.0),
            color: (255, 0, 0, 255),
            stroke_width: 4.0,
            is_filled: false,
        }));

        // 滚轮向上滚动：4.0 -> 5.0
        let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
        let changed = handle_mouse_wheel(&mut state, delta);
        assert!(changed);
        assert_eq!(state.toolbar.sub_options.custom_stroke_width, 5.0);

        // 再次向上：5.0 -> 6.0，档位变为 Thick
        let changed = handle_mouse_wheel(&mut state, delta);
        assert!(changed);
        assert_eq!(state.toolbar.sub_options.custom_stroke_width, 6.0);
        assert_eq!(
            state.toolbar.sub_options.stroke_width,
            StrokeWidthLevel::Thick
        );
    }

    #[test]
    fn test_mouse_wheel_text_mode_font_size_adjust() {
        let mut state = SnipperState::new();
        state.toolbar.active_tool = Some(ToolbarItem::ToolText);
        let initial_sz = state.toolbar.sub_options.font_size;

        // 处于文字模式，向上滚字号增大 2.0
        let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
        let changed = handle_mouse_wheel(&mut state, delta);
        assert!(changed);
        assert_eq!(state.toolbar.sub_options.font_size, initial_sz + 2.0);
    }
}
