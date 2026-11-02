//! 鼠标按键按下、释放与自动后处理触发。

use super::*;

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
                crate::snipper::window::redaction::toggle_redaction_region(state, rect);
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
            crate::snipper::window::actions::handle_action_pick_color(state, original_rgba);
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
pub(super) fn handle_left_press(
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
        crate::snipper::window::annotation_edit::clear(state);
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
        crate::snipper::window::annotation_edit::clear(state);
        if state.toolbar.begin_drag(cx, cy) {
            return true;
        }
        if let Some(item) = state.toolbar.handle_click(cx, cy) {
            dispatch_toolbar_action(state, item, original_rgba);
        }
        return true;
    }

    // 点击画布表示离开二级栏数值控件；后续滚轮恢复默认粗细调节。
    state.toolbar.clear_sub_focus();

    commit_active_text_input(state);

    // 已提交标注的编辑优先于新建文本与新图元：默认只允许编辑最新一条，
    // 旧标注处于锁定态，让它们重新变成可绘制的画布；按住 Alt 可显式解锁旧标注。
    let allow_locked_annotation = state.modifiers.alt_key();
    if crate::snipper::window::annotation_edit::handle_press(state, cx, cy, allow_locked_annotation)
    {
        return true;
    }

    // 未激活绘制工具时点击旧标注：不应误拖动整个选区，改为提示 Alt 解锁方式。
    if state.toolbar.active_tool.is_none()
        && crate::snipper::window::annotation_edit::hit_test_locked(state, (cx, cy))
    {
        state.set_status_message(snapin_foundation::i18n::tr(
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
        crate::snipper::window::annotation_edit::clear(state);
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
            crate::snipper::window::annotation_edit::clear(state);
            state.stroke.points.push((cx, cy));
            state.stroke.is_drawing = true;
            return true;
        }
        if state.toolbar.active_tool.is_some() {
            crate::snipper::window::annotation_edit::clear(state);
            state.stroke.points.clear();
            state.stroke.points.push((cx, cy));
            return true;
        }
        crate::snipper::window::annotation_edit::clear(state);
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
    crate::snipper::window::annotation_edit::clear(state);
    state.selection = SelectionState::Creating {
        start: (cx, cy),
        current: (cx, cy),
    };
    true
}

/// 鼠标左键抬起交互
pub(super) fn handle_left_release(
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
    if crate::snipper::window::annotation_edit::handle_drag_release(state) {
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
            snapin_foundation::PostCaptureAction::SmartRedact => {
                crate::snipper::window::actions::handle_action_redact(state, original_rgba);
            }
            snapin_foundation::PostCaptureAction::Qr => {
                crate::snipper::window::actions::handle_action_qr(state, original_rgba);
            }
            snapin_foundation::PostCaptureAction::Beautify => {
                crate::snipper::window::actions::handle_action_save_beautified(
                    state,
                    original_rgba,
                );
            }
            snapin_foundation::PostCaptureAction::OcrPanel => {
                crate::snipper::window::actions::handle_action_ocr(state, original_rgba);
            }
        }
    } else if state.mode == SnipMode::AutoCopy {
        crate::snipper::window::actions::handle_action_copy(state, original_rgba);
    } else if state.mode == SnipMode::Translate {
        state.mode = SnipMode::Normal;
        crate::snipper::window::actions::handle_action_translate(state, original_rgba);
    }
}
