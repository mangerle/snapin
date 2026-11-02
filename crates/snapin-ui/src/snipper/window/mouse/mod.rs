//! 截图交互鼠标移动、按键点击、滚轮缩放与选区拖拽处理。

pub mod toolbar_action;

mod eraser;

use super::annotation_builder::commit_annotation;
use super::events::{commit_active_text_input, handle_escape, SnipperState};
use super::state::window_at;
use super::types::SnipMode;
use crate::editor::toolbar::{LineMode, StrokeWidthLevel, Tool};
use crate::model::ShapeId;
use crate::snipper::selection::{hit_test_handle, DragMode, Rect, SelectionState};
use image::RgbaImage;
use snapin_core::annotation::AnnotationShape;
use toolbar_action::{dispatch_toolbar_action, handle_text_tool_click};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

/// 鼠标移动事件统一处理 (更新光标坐标、选区创建与调整手柄拖拽)
pub fn handle_cursor_move(
    state: &mut SnipperState,
    pos: (f64, f64),
    screen_w: f32,
    screen_h: f32,
) -> bool {
    state.cursor_pos = pos;
    state.cursor_ready = true;
    let (cx, cy) = (pos.0 as f32, pos.1 as f32);

    // 标注编辑拖拽优先于选区与工具栏悬停处理
    if state.annotation_drag.is_some() {
        return super::annotation_edit::handle_drag_move(state, cx, cy);
    }
    if state.toolbar.is_dragging() {
        return state.toolbar.drag_to(cx, cy);
    }

    match &mut state.selection {
        SelectionState::Idle => {
            let hovered = window_at((cx, cy), &state.window_rects);
            let changed = hovered != state.hovered_window;
            state.hovered_window = hovered;
            if changed {
                state.window_cycle = None;
            }
            // 放大镜必须逐帧跟随光标；禁用放大镜时仍只在高亮窗口切换时重绘
            changed || state.magnifier.enabled
        }
        SelectionState::Creating { start, current } => {
            *current = state.selection_constraint.constrain(*start, (cx, cy));
            true
        }
        SelectionState::Locked { rect, active_drag } => {
            if let Some(drag) = active_drag {
                let (r, d) = super::drag_calculator::calculate_drag(rect, drag, (cx, cy));
                *rect = r;
                *active_drag = Some(d);
                state.toolbar.update_layout(rect, screen_w, screen_h);
                true
            } else if state.toolbar.active_tool == Some(Tool::Eraser) {
                let px = cx.clamp(rect.x, rect.x + rect.width);
                let py = cy.clamp(rect.y, rect.y + rect.height);
                eraser::handle_drag_move(state, (px, py));
                true
            } else if state.toolbar.active_tool.is_some() && !state.stroke.points.is_empty() {
                let px = cx.clamp(rect.x, rect.x + rect.width);
                let py = cy.clamp(rect.y, rect.y + rect.height);
                let tool = state.toolbar.active_tool;
                match tool {
                    Some(Tool::Pen) if state.modifiers.shift_key() => {
                        let start = state.stroke.points[0];
                        let endpoint = constrain_with_shift(Tool::Arrow, start, (px, py));
                        state.stroke.points.truncate(1);
                        state.stroke.points.push(endpoint);
                    }
                    Some(Tool::Pen) | Some(Tool::Marker) => {
                        state.stroke.points.push((px, py));
                    }
                    Some(Tool::Mosaic) if state.toolbar.sub_options.mosaic_brush_mode => {
                        if state.stroke.points.last() != Some(&(px, py)) {
                            state.stroke.points.push((px, py));
                        }
                    }
                    Some(Tool::Line)
                        if state.toolbar.sub_options.line_mode == LineMode::Polyline =>
                    {
                        // 折线由连续单击追加节点，鼠标移动只刷新预览，不修改已提交节点。
                    }
                    _ => {
                        let endpoint = match (state.modifiers.shift_key(), tool) {
                            (true, Some(active)) => {
                                let start = state.stroke.points[0];
                                constrain_with_shift(active, start, (px, py))
                            }
                            _ => (px, py),
                        };
                        if state.stroke.points.len() > 1 {
                            state.stroke.points[1] = endpoint;
                        } else {
                            state.stroke.points.push(endpoint);
                        }
                    }
                }
                true
            } else {
                let hover_changed = state.toolbar.handle_mouse_move(cx, cy);
                hover_changed || has_brush_cursor(state)
            }
        }
    }
}

/// 按住 Shift 时的等比约束：矩形/椭圆取正方形，箭头吸附到 45° 的整数倍方向。
///
/// 起点保持不变，终点随约束移动；矩形取两次位移中较大的边长，保证拖拽方向与用户意图一致。
fn constrain_with_shift(tool: Tool, start: (f32, f32), current: (f32, f32)) -> (f32, f32) {
    let (x0, y0) = start;
    let (mut x, mut y) = current;
    let dx = x - x0;
    let dy = y - y0;

    match tool {
        Tool::Rect | Tool::Ellipse | Tool::Blur | Tool::Spotlight | Tool::Magnifier => {
            let side = dx.abs().max(dy.abs());
            x = x0 + side * dx.signum();
            y = y0 + side * dy.signum();
        }
        Tool::Arrow => {
            let length = (dx * dx + dy * dy).sqrt();
            if length > 0.5 {
                let step = std::f32::consts::FRAC_PI_4;
                let angle = (dy.atan2(dx) / step).round() * step;
                x = x0 + angle.cos() * length;
                y = y0 + angle.sin() * length;
            }
        }
        _ => {}
    }

    (x, y)
}

/// 当前工具是否需要持续跟随光标重绘圆形画笔轮廓。
fn has_brush_cursor(state: &SnipperState) -> bool {
    match state.toolbar.active_tool {
        Some(Tool::Mosaic) => state.toolbar.sub_options.mosaic_brush_mode,
        Some(Tool::Eraser) => !state.toolbar.sub_options.eraser_rectangle_mode,
        _ => false,
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

    if button == MouseButton::Left {
        let hit_region = state
            .redaction_preview
            .as_ref()
            .and_then(|preview| preview.hit_at((cx, cy)));
        if let Some(rect) = hit_region {
            if button_state == ElementState::Pressed {
                super::redaction::toggle_redaction_region(state, rect);
            }
            return true;
        }
    }

    if button == MouseButton::Right && button_state == ElementState::Pressed {
        if state.toolbar.active_tool == Some(Tool::Line) && state.stroke.points.len() >= 2 {
            commit_annotation(state);
            return true;
        }
        return handle_escape(state);
    }

    if state.mode == SnipMode::PickColor {
        if button == MouseButton::Left && button_state == ElementState::Pressed {
            super::actions::handle_action_pick_color(state, original_rgba);
            return true;
        }
        return false;
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
    if state.ocr_panel.is_some() {
        let scale = state.toolbar.ui_scale;
        let handled = state.ocr_panel.as_mut().is_some_and(|panel| {
            panel.handle_click(
                cx,
                cy,
                original_rgba.width() as f32,
                original_rgba.height() as f32,
                scale,
            )
        });
        if handled {
            commit_active_text_input(state);
            return true;
        }
    }

    if let SelectionState::Locked { rect, .. } = state.selection {
        return handle_locked_press(state, rect, cx, cy, original_rgba);
    }

    if matches!(state.selection, SelectionState::Idle) {
        state.press_window = state.hovered_window;
        super::annotation_edit::clear(state);
        state.toolbar.reset_manual_position();
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
        super::annotation_edit::clear(state);
        if state.toolbar.begin_drag(cx, cy) {
            return true;
        }
        if let Some(item) = state.toolbar.handle_click(cx, cy) {
            dispatch_toolbar_action(state, item, original_rgba);
        }
        return true;
    }

    commit_active_text_input(state);

    // 已提交标注的编辑优先于新建文本与新图元：默认只允许编辑最新一条，
    // 旧标注处于锁定态，让它们重新变成可绘制的画布；按住 Alt 可显式解锁旧标注。
    let allow_locked_annotation = state.modifiers.alt_key();
    if super::annotation_edit::handle_press(state, cx, cy, allow_locked_annotation) {
        return true;
    }

    // 未激活绘制工具时点击旧标注：不应误拖动整个选区，改为提示 Alt 解锁方式。
    if state.toolbar.active_tool.is_none()
        && super::annotation_edit::hit_test_locked(state, (cx, cy))
    {
        state.set_status_message(snapin_core::i18n::tr(
            "旧标注已锁定，按住 Alt 点击可重新编辑",
        ));
        return true;
    }

    if state.toolbar.active_tool == Some(Tool::Text) {
        return handle_text_tool_click(state, cx, cy);
    }

    // 选区控制手柄优先于继续绘制：工具激活时也允许随时调整选区大小
    let handle_tolerance = 8.0 * state.toolbar.ui_scale;
    if let Some(handle) = hit_test_handle(&rect, cx, cy, handle_tolerance) {
        super::annotation_edit::clear(state);
        if let SelectionState::Locked {
            ref mut active_drag,
            ..
        } = state.selection
        {
            *active_drag = Some(DragMode::Resize(handle));
        }
        return true;
    }

    if rect.contains(cx, cy) {
        if state.toolbar.active_tool == Some(Tool::Eraser) {
            return eraser::handle_press(state, cx, cy);
        }
        if state.toolbar.active_tool == Some(Tool::Line)
            && state.toolbar.sub_options.line_mode == LineMode::Polyline
        {
            super::annotation_edit::clear(state);
            state.stroke.points.push((cx, cy));
            state.stroke.is_drawing = true;
            return true;
        }
        if state.toolbar.active_tool.is_some() {
            super::annotation_edit::clear(state);
            state.stroke.points.clear();
            state.stroke.points.push((cx, cy));
            return true;
        }
        super::annotation_edit::clear(state);
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

    // 点击在已有选区外部：清除当前选区，允许重新框选；工具栏随新选区重新自适应摆放
    state.toolbar.active_tool = None;
    state.toolbar.reset_manual_position();
    state.stroke.points.clear();
    super::annotation_edit::clear(state);
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
    let remember_selection = state.config.snip.remember_last_selection;
    let desktop = state.desktop.clone();
    let pressed_window = state.press_window.take();

    // 标注拖动或手柄拉伸结束：编辑结果已实时写入历史栈，直接消费本次释放
    if super::annotation_edit::handle_drag_release(state) {
        return true;
    }
    // 工具栏拖动结束：位置已在移动中实时更新，直接消费本次释放
    if state.toolbar.end_drag() {
        return true;
    }

    match &mut state.selection {
        SelectionState::Creating { start, current } => {
            // 短按未拖动且按下时命中了窗口：直接吸附到该窗口；拖动则仍走自由框选。
            let drag_x = (current.0 - start.0).abs();
            let drag_y = (current.1 - start.1).abs();
            if drag_x <= 3.0 && drag_y <= 3.0 {
                if let Some(window_rect) = pressed_window {
                    if window_rect.width > 4.0 && window_rect.height > 4.0 {
                        if remember_selection {
                            desktop.save_last_selection(window_rect);
                        }
                        state.history.push(window_rect);
                        state
                            .toolbar
                            .update_layout(&window_rect, screen_w, screen_h);
                        state.selection = SelectionState::Locked {
                            rect: window_rect,
                            active_drag: None,
                        };
                        state.hovered_window = None;
                        apply_auto_mode(state, original_rgba);
                        return true;
                    }
                }
            }

            let rect = Rect::from_points(*start, *current);
            if rect.width > 4.0 && rect.height > 4.0 {
                if remember_selection {
                    desktop.save_last_selection(rect);
                }
                state.history.push(rect);
                state.toolbar.update_layout(&rect, screen_w, screen_h);
                state.selection = SelectionState::Locked {
                    rect,
                    active_drag: None,
                };
                apply_auto_mode(state, original_rgba);
            } else {
                state.selection = SelectionState::Idle;
            }
            true
        }
        SelectionState::Locked { rect, active_drag } => {
            // 若释放位置在工具栏区域内，清理暂存点并直接拦截，避免误提交标注
            if state.toolbar.contains_point(cx, cy) {
                state.stroke.points.clear();
                return true;
            }

            if active_drag.is_some() {
                *active_drag = None;
                if remember_selection {
                    desktop.save_last_selection(*rect);
                }
                state.history.push(*rect);
                state.toolbar.update_layout(rect, screen_w, screen_h);
                true
            } else if state.toolbar.active_tool == Some(Tool::Eraser) {
                eraser::handle_release(state, cx, cy)
            } else if state.toolbar.active_tool == Some(Tool::Line)
                && state.toolbar.sub_options.line_mode == LineMode::Polyline
            {
                // 折线在右键或 Enter 时提交，单击释放只追加节点。
                true
            } else if state.toolbar.active_tool.is_some() && !state.stroke.points.is_empty() {
                state.stroke.points.push((cx, cy));
                commit_annotation(state);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// 选区锁定后执行一次性自动复制或自动翻译。
///
/// 自动翻译只对本次热键触发的首次框选生效：先复位模式，避免用户之后重新框选时
/// 再次触发整图翻译；翻译完成后保留工具栏继续交互。
fn apply_auto_mode(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let Some(action) = state.post_action.take() {
        match action {
            snapin_core::PostCaptureAction::SmartRedact => {
                super::actions::handle_action_redact(state, original_rgba);
            }
            snapin_core::PostCaptureAction::Qr => {
                super::actions::handle_action_qr(state, original_rgba);
            }
            snapin_core::PostCaptureAction::Beautify => {
                super::actions::handle_action_save_beautified(state, original_rgba);
            }
            snapin_core::PostCaptureAction::OcrPanel => {
                super::actions::handle_action_ocr(state, original_rgba);
            }
        }
    } else if state.mode == SnipMode::AutoCopy {
        super::actions::handle_action_copy(state, original_rgba);
    } else if state.mode == SnipMode::Translate {
        state.mode = SnipMode::Normal;
        super::actions::handle_action_translate(state, original_rgba);
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

    if state.toolbar.active_tool == Some(Tool::Magnifier)
        && !state
            .toolbar
            .contains_point(state.cursor_pos.0 as f32, state.cursor_pos.1 as f32)
        && adjust_magnifier_wheel(state, delta_y)
    {
        return true;
    }

    if state.toolbar.handle_sub_wheel(
        state.cursor_pos.0 as f32,
        state.cursor_pos.1 as f32,
        delta_y,
    ) {
        if let Some(session) = &mut state.active_text_input {
            session.font_size = state.toolbar.sub_options.font_size;
        }
        if state.toolbar.active_tool == Some(Tool::Magnifier) {
            sync_selected_magnifier_scale(state);
        }
        return true;
    }

    let is_text_mode =
        state.toolbar.active_tool == Some(Tool::Text) || state.active_text_input.is_some();

    if is_text_mode {
        let step = if delta_y > 0.0 { 2.0 } else { -2.0 };
        let new_font_size = (state.toolbar.sub_options.font_size + step).clamp(12.0, 72.0);
        state.toolbar.sub_options.font_size = new_font_size;

        if let Some(session) = &mut state.active_text_input {
            session.font_size = new_font_size;
        }

        state.annotations.adjust_last_font_size(new_font_size);
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

        state.annotations.adjust_last_stroke_width(new_stroke_w);
        true
    }
}

/// 根据光标是否位于放大原区域或放大图像内，调整倍率或边框粗细。
fn adjust_magnifier_wheel(state: &mut SnipperState, delta_y: f32) -> bool {
    let index = state
        .selected_annotation
        .map(ShapeId::get)
        .or_else(|| state.annotations.undo_stack().len().checked_sub(1));
    let point = (state.cursor_pos.0 as f32, state.cursor_pos.1 as f32);
    let mut over_magnifier = false;
    if let Some(index) = index {
        if let Some(AnnotationShape::Magnifier(action)) = state.annotations.shape_at(index) {
            let source = Rect {
                x: action.source.0,
                y: action.source.1,
                width: action.source.2,
                height: action.source.3,
            };
            let dest = Rect {
                x: action.dest.0,
                y: action.dest.1,
                width: action.dest.2,
                height: action.dest.3,
            };
            over_magnifier = source.contains(point.0, point.1) || dest.contains(point.0, point.1);
        }
    }

    if over_magnifier {
        let step = if delta_y > 0.0 { 0.25 } else { -0.25 };
        state.toolbar.sub_options.magnifier_scale =
            (state.toolbar.sub_options.magnifier_scale + step).clamp(1.0, 8.0);
        sync_selected_magnifier_scale(state);
    } else {
        let step = if delta_y > 0.0 { 1.0 } else { -1.0 };
        state.toolbar.sub_options.custom_stroke_width =
            (state.toolbar.sub_options.custom_stroke_width + step).clamp(1.0, 48.0);
        if let Some(index) = index {
            if let Some(mut shape) = state.annotations.shape_at(index).cloned() {
                shape.adjust_size(snapin_core::annotation::shapes::SizeParam::Stroke(
                    state.toolbar.sub_options.custom_stroke_width,
                ));
                state.annotations.replace(index, shape);
            }
        }
    }
    true
}

/// 将工具栏倍率同步到当前选中的放大镜图元。
fn sync_selected_magnifier_scale(state: &mut SnipperState) {
    let Some(index) = state
        .selected_annotation
        .map(ShapeId::get)
        .or_else(|| state.annotations.undo_stack().len().checked_sub(1))
    else {
        return;
    };
    let Some(mut shape) = state.annotations.shape_at(index).cloned() else {
        return;
    };
    let AnnotationShape::Magnifier(action) = &mut shape else {
        return;
    };
    let center = (
        action.dest.0 + action.dest.2 / 2.0,
        action.dest.1 + action.dest.3 / 2.0,
    );
    action.adjust_scale(state.toolbar.sub_options.magnifier_scale, Some(center));
    state.annotations.replace(index, shape);
}

#[cfg(test)]
mod tests;
