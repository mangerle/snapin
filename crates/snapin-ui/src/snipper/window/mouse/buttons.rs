//! 鼠标按键按下、释放与自动后处理触发。

use super::*;
use crate::editor::toolbar::SubControl;
use crate::geometry::Point;
use crate::snipper::selection::ResizeHandle;

/// 鼠标按键点击事件处理
pub fn handle_mouse_button(
    state: &mut SnipperState,
    button: MouseButton,
    button_state: ElementState,
    original_rgba: &RgbaImage,
) -> bool {
    let (cx, cy) = (
        state.viewport.cursor_pos.0 as f32,
        state.viewport.cursor_pos.1 as f32,
    );

    let event = super::PointerButtonEvent {
        button,
        button_state,
        position: (cx, cy),
        original_rgba,
    };

    if let Some(handled) = super::ocr::handle_ocr_overlay_press(state, &event) {
        return handled;
    }
    if let Some(handled) = super::recording::handle_recording_button(state, event) {
        return handled;
    }

    if button == MouseButton::Left {
        let hit_region = state
            .ocr
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
        if state.editor.annotation.toolbar.active_tool == Some(Tool::Line)
            && state.editor.annotation.stroke.points.len() >= 2
        {
            commit_annotation(state);
            return true;
        }
        return handle_escape(state);
    }

    if state.session.mode == SnipMode::PickColor {
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
        ElementState::Released => {
            let handled = handle_left_release(state, cx, cy, original_rgba);
            state.sync_recording_overlay();
            handled
        }
    }
}

/// 鼠标左键按下交互
pub(super) fn handle_left_press(
    state: &mut SnipperState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> bool {
    if let SelectionState::Locked { rect, .. } = state.viewport.selection {
        return handle_locked_press(state, rect, Point::new(cx, cy), original_rgba);
    }

    if matches!(state.viewport.selection, SelectionState::Idle) {
        state.viewport.press_window = state.viewport.hovered_window;
        crate::snipper::window::annotation_edit::clear(state);
        state.editor.annotation.toolbar.reset_manual_position();
        state.viewport.selection = SelectionState::Creating {
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
    pointer: Point,
    original_rgba: &RgbaImage,
) -> bool {
    // 优先检测是否点击在工具栏范围内，坚决拦截阻止事件穿透到画布
    if state
        .editor
        .annotation
        .toolbar
        .contains_point(pointer.x, pointer.y)
    {
        return handle_toolbar_press(state, pointer.x, pointer.y, original_rgba);
    }

    // 判定双击事件：350毫秒内且光标位移在 6 像素容差范围内
    let now = std::time::Instant::now();
    let is_double_click = state
        .editor
        .last_click
        .is_some_and(|(prev_time, prev_pos)| {
            now.duration_since(prev_time).as_millis() < 350
                && (prev_pos.0 - pointer.x).abs() < 6.0
                && (prev_pos.1 - pointer.y).abs() < 6.0
        });
    state.editor.last_click = Some((now, (pointer.x, pointer.y)));

    // 点击画布表示离开二级栏数值控件；后续滚轮恢复默认粗细调节。
    state.editor.annotation.toolbar.clear_sub_focus();

    // 若当前正在输入文字且点击落在输入框内，将光标定位到点击字符并保留会话
    if let Some(session) = &mut state.editor.annotation.active_text_input {
        if session.contains_screen_point(pointer.x, pointer.y, 1.0) {
            let shift = state.viewport.modifiers.shift_key();
            session.set_cursor_from_screen_point(pointer.x, pointer.y, shift, 1.0);
            state.editor.annotation.is_selecting_text = true;
            return true;
        }
    }
    commit_active_text_input(state);

    if let Some(handled) =
        super::annotation::handle_annotation_press(state, pointer.x, pointer.y, is_double_click)
    {
        return handled;
    }
    if let Some(handle) = selection_handle_at(state, rect, pointer.x, pointer.y) {
        return begin_selection_resize(state, handle);
    }
    if rect.contains(pointer.x, pointer.y) {
        return handle_inside_selection_press(state, rect, pointer.x, pointer.y);
    }
    reset_selection_from_canvas(state, pointer.x, pointer.y);
    true
}

/// 处理工具栏范围内的按下；无论是否命中控件都拦截穿透。
fn handle_toolbar_press(
    state: &mut SnipperState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> bool {
    commit_active_text_input(state);
    let prev_tool = state.editor.annotation.toolbar.active_tool;
    if state.editor.annotation.toolbar.begin_drag(cx, cy) {
        crate::snipper::window::annotation_edit::clear(state);
        return true;
    }
    if let Some(item) = state.editor.annotation.toolbar.handle_click(cx, cy) {
        dispatch_toolbar_action(state, item, original_rgba);
    }
    if state.editor.annotation.toolbar.active_tool != prev_tool {
        crate::snipper::window::annotation_edit::clear(state);
    }
    if let Some(index) = state
        .editor
        .selected_annotation
        .map(crate::model::ShapeId::get)
    {
        if matches!(
            state.editor.annotations.shape_at(index),
            Some(AnnotationShape::Text(_) | AnnotationShape::Translation(_))
        ) {
            state
                .editor
                .annotations
                .adjust_font_size_at(index, state.editor.annotation.toolbar.sub_options.font_size);
        }
    }
    if state.editor.annotation.toolbar.focused_sub_control() == Some(SubControl::EffectStrength) {
        crate::snipper::window::redaction::sync_redaction_style(state, SubControl::EffectStrength);
    }
    true
}

/// 查找选区控制手柄；工具激活时也允许随时调整选区大小。
fn selection_handle_at(state: &SnipperState, rect: Rect, cx: f32, cy: f32) -> Option<ResizeHandle> {
    let handle_tolerance = 8.0 * state.editor.annotation.toolbar.ui_scale;
    hit_test_handle(&rect, cx, cy, handle_tolerance)
}

/// 进入选区手柄缩放状态。
fn begin_selection_resize(state: &mut SnipperState, handle: ResizeHandle) -> bool {
    crate::snipper::window::annotation_edit::clear(state);
    if let SelectionState::Locked {
        ref mut active_drag,
        ..
    } = state.viewport.selection
    {
        *active_drag = Some(DragMode::Resize(handle));
    }
    true
}

/// 处理选区内部的橡皮擦、绘制工具或整体拖动。
fn handle_inside_selection_press(state: &mut SnipperState, rect: Rect, cx: f32, cy: f32) -> bool {
    if state.editor.annotation.toolbar.active_tool == Some(Tool::Eraser) {
        return eraser::handle_press(state, cx, cy);
    }
    if state.editor.annotation.toolbar.active_tool == Some(Tool::Line)
        && state.editor.annotation.toolbar.sub_options.line_mode == LineMode::Polyline
    {
        crate::snipper::window::annotation_edit::clear(state);
        state
            .editor
            .annotation
            .stroke
            .append(crate::geometry::Point::new(cx, cy));
        state.editor.annotation.stroke.is_drawing = true;
        return true;
    }
    if state.editor.annotation.toolbar.active_tool.is_some() {
        crate::snipper::window::annotation_edit::clear(state);
        state
            .editor
            .annotation
            .stroke
            .begin(crate::geometry::Point::new(cx, cy));
        return true;
    }
    crate::snipper::window::annotation_edit::clear(state);
    if let SelectionState::Locked {
        ref mut active_drag,
        ..
    } = state.viewport.selection
    {
        *active_drag = Some(DragMode::Move {
            offset_x: (cx - rect.x) as i32,
            offset_y: (cy - rect.y) as i32,
        });
    }
    true
}

/// 点击空白画布时清除旧选区并开始新的框选。
fn reset_selection_from_canvas(state: &mut SnipperState, cx: f32, cy: f32) {
    // 点击在已有选区外部：清除当前选区，允许重新框选；工具栏随新选区重新自适应摆放
    state.editor.annotation.toolbar.active_tool = None;
    state.editor.annotation.toolbar.reset_manual_position();
    state.editor.annotation.stroke.points.clear();
    crate::snipper::window::annotation_edit::clear(state);
    state.viewport.selection = SelectionState::Creating {
        start: (cx, cy),
        current: (cx, cy),
    };
}

/// 鼠标左键抬起交互
pub(super) fn handle_left_release(
    state: &mut SnipperState,
    cx: f32,
    cy: f32,
    original_rgba: &RgbaImage,
) -> bool {
    let release = PointerRelease {
        cx,
        cy,
        original_rgba,
        pressed_window: state.viewport.press_window.take(),
    };

    let was_selecting = state.editor.annotation.is_selecting_text;
    state.editor.annotation.is_selecting_text = false;
    if was_selecting {
        return true;
    }

    // 标注拖动或手柄拉伸结束：编辑结果已实时写入历史栈，直接消费本次释放
    if finish_drag_interactions(state) {
        return true;
    }

    if let SelectionState::Creating { start, current } = state.viewport.selection {
        return finish_creating_selection(state, start, current, &release);
    }
    if let SelectionState::Locked { rect, active_drag } = state.viewport.selection {
        return finish_locked_selection(state, rect, active_drag, &release);
    }
    false
}

/// 左键释放时仍需结束的标注或工具栏拖动。
struct PointerRelease<'a> {
    cx: f32,
    cy: f32,
    original_rgba: &'a RgbaImage,
    pressed_window: Option<Rect>,
}

/// 完成标注编辑与工具栏拖动的释放阶段。
fn finish_drag_interactions(state: &mut SnipperState) -> bool {
    if crate::snipper::window::annotation_edit::handle_drag_release(state) {
        return true;
    }
    state.editor.annotation.toolbar.end_drag()
}

/// 完成初始框选，短按时优先吸附到按下时命中的窗口。
fn finish_creating_selection(
    state: &mut SnipperState,
    start: (f32, f32),
    current: (f32, f32),
    release: &PointerRelease<'_>,
) -> bool {
    let drag_x = (current.0 - start.0).abs();
    let drag_y = (current.1 - start.1).abs();
    if drag_x <= 3.0 && drag_y <= 3.0 {
        if let Some(window_rect) = release.pressed_window {
            if window_rect.width > 4.0 && window_rect.height > 4.0 {
                lock_selection(state, window_rect, release, true);
                apply_auto_mode(state, release.original_rgba);
                return true;
            }
        }
    }

    let rect = Rect::from_points(start, current);
    if rect.width > 4.0 && rect.height > 4.0 {
        lock_selection(state, rect, release, false);
        apply_auto_mode(state, release.original_rgba);
    } else {
        state.viewport.selection = SelectionState::Idle;
    }
    true
}

/// 持久化并锁定一个有效选区。
fn lock_selection(
    state: &mut SnipperState,
    rect: Rect,
    release: &PointerRelease<'_>,
    clear_hovered_window: bool,
) {
    if state.session.config.snip.remember_last_selection {
        state.session.desktop.save_last_selection(rect);
    }
    state.viewport.history.push(rect);
    state.editor.annotation.toolbar.update_layout(
        &rect,
        release.original_rgba.width() as f32,
        release.original_rgba.height() as f32,
    );
    state.viewport.selection = SelectionState::Locked {
        rect,
        active_drag: None,
    };
    if clear_hovered_window {
        state.viewport.hovered_window = None;
    }
}

/// 完成锁定选区内的拖动、橡皮擦或绘制提交。
fn finish_locked_selection(
    state: &mut SnipperState,
    rect: Rect,
    active_drag: Option<DragMode>,
    release: &PointerRelease<'_>,
) -> bool {
    // 若释放位置在工具栏区域内，清理暂存点并直接拦截，避免误提交标注
    if state
        .editor
        .annotation
        .toolbar
        .contains_point(release.cx, release.cy)
    {
        state.editor.annotation.stroke.points.clear();
        return true;
    }

    if active_drag.is_some() {
        finish_locked_drag(state, rect, release);
        return true;
    }
    if state.editor.annotation.toolbar.active_tool == Some(Tool::Eraser) {
        return eraser::handle_release(state, release.cx, release.cy);
    }
    if state.editor.annotation.toolbar.active_tool == Some(Tool::Line)
        && state.editor.annotation.toolbar.sub_options.line_mode == LineMode::Polyline
    {
        // 折线在右键或 Enter 时提交，单击释放只追加节点。
        return true;
    }
    if state.editor.annotation.toolbar.active_tool.is_some()
        && !state.editor.annotation.stroke.is_empty()
    {
        state
            .editor
            .annotation
            .stroke
            .points
            .push(crate::geometry::Point::new(release.cx, release.cy));
        commit_annotation(state);
        return true;
    }
    false
}

/// 完成选区整体移动或手柄缩放后的落点保存。
fn finish_locked_drag(state: &mut SnipperState, rect: Rect, release: &PointerRelease<'_>) {
    if let SelectionState::Locked { active_drag, .. } = &mut state.viewport.selection {
        *active_drag = None;
    }
    if state.session.config.snip.remember_last_selection {
        state.session.desktop.save_last_selection(rect);
    }
    state.viewport.history.push(rect);
    state.editor.annotation.toolbar.update_layout(
        &rect,
        release.original_rgba.width() as f32,
        release.original_rgba.height() as f32,
    );
}

/// 选区锁定后执行一次性自动复制或自动翻译。
///
/// 自动翻译只对本次热键触发的首次框选生效：先复位模式，避免用户之后重新框选时
/// 再次触发整图翻译；翻译完成后保留工具栏继续交互。
fn apply_auto_mode(state: &mut SnipperState, original_rgba: &RgbaImage) {
    if let Some(action) = state.session.post_action.take() {
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
                crate::snipper::window::actions::handle_action_ocr_edit(state, original_rgba);
            }
        }
    } else if state.session.mode == SnipMode::AutoCopy {
        crate::snipper::window::actions::handle_action_copy(state, original_rgba);
    } else if state.session.mode == SnipMode::Translate {
        state.session.mode = SnipMode::Normal;
        crate::snipper::window::actions::handle_action_translate(state, original_rgba);
    }
}
