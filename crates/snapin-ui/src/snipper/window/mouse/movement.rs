//! 鼠标移动、选区约束与画笔光标判定。

use super::*;

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
        return crate::snipper::window::annotation_edit::handle_drag_move(state, cx, cy);
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
                let (r, d) =
                    crate::snipper::window::drag_calculator::calculate_drag(rect, drag, (cx, cy));
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
pub(super) fn constrain_with_shift(
    tool: Tool,
    start: (f32, f32),
    current: (f32, f32),
) -> (f32, f32) {
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
