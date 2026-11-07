//! 鼠标移动、选区约束与画笔光标判定。

use super::*;

/// 鼠标移动事件统一处理 (更新光标坐标、选区创建与调整手柄拖拽)
pub fn handle_cursor_move(
    state: &mut SnipperState,
    pos: (f64, f64),
    screen_w: f32,
    screen_h: f32,
) -> bool {
    state.viewport.cursor_pos = pos;
    state.viewport.cursor_ready = true;
    if state.ocr.editor.is_some() {
        return handle_editor_cursor_move(state, pos.0 as f32, pos.1 as f32, screen_w, screen_h);
    }
    if let Some(menu) = state.ocr.menu.as_mut() {
        return menu.update_hover(pos.0 as f32, pos.1 as f32);
    }
    let frame = CursorFrame {
        x: pos.0 as f32,
        y: pos.1 as f32,
        screen_width: screen_w,
        screen_height: screen_h,
    };

    // 标注编辑拖拽优先于选区与工具栏悬停处理
    if state.editor.annotation.is_selecting_text {
        if let Some(session) = &mut state.editor.annotation.active_text_input {
            session.set_cursor_from_screen_point(frame.x, frame.y, true, 1.0);
            return true;
        }
    }
    if state.editor.annotation_drag.is_some() {
        return crate::snipper::window::annotation_edit::handle_drag_move(state, frame.x, frame.y);
    }
    if state.editor.annotation.toolbar.is_dragging() {
        return state.editor.annotation.toolbar.drag_to(frame.x, frame.y);
    }

    match state.viewport.selection {
        SelectionState::Idle => handle_idle_cursor(state, frame),
        SelectionState::Creating { start, current } => {
            handle_creating_cursor(state, start, current, frame)
        }
        SelectionState::Locked { rect, active_drag } => {
            handle_locked_cursor(state, rect, active_drag, frame)
        }
    }
}

/// 处理文字识别全文编辑框中的鼠标移动（窗口拖拽、文本拖选、滚动条拖拽与按钮悬停）。
fn handle_editor_cursor_move(
    state: &mut SnipperState,
    x: f32,
    y: f32,
    screen_w: f32,
    screen_h: f32,
) -> bool {
    let scale = state.editor.annotation.toolbar.ui_scale;
    let Some(editor) = state.ocr.editor.as_mut() else {
        return false;
    };
    let layout = editor.layout(screen_w, screen_h, scale);

    // 1. 拖动面板
    if let Some((offset_x, offset_y)) = editor.drag_start {
        let new_x = x - offset_x;
        let new_y = y - offset_y;
        editor.custom_position = Some((new_x, new_y));
        return true;
    }

    // 2. 拖选文本
    if editor.is_selecting_text {
        let padding_x = 12.0 * scale;
        let padding_y = 10.0 * scale;
        let max_text_width = (layout.body.width - padding_x * 2.0 - 12.0 * scale).max(10.0);
        let options = snapin_render::TextMeasureOptions::new(
            14.0 * scale,
            22.0 * scale,
            max_text_width,
            10000.0,
        );
        let hit_x = (x - layout.body.x - padding_x).max(0.0);
        let hit_y = (y - layout.body.y - padding_y + editor.scroll_y).max(0.0);
        let text = editor.display_text();
        let offset = snapin_render::with_thread_local_text_renderer(|renderer| {
            renderer.hit_test_text_offset(&text, hit_x, hit_y, options)
        });
        if editor.cursor != offset {
            editor.cursor = offset;
            editor.reset_blink();
            return true;
        }
        return false;
    }

    // 3. 拖动滚动条
    if editor.is_dragging_scrollbar {
        if let (Some(track), Some(thumb)) = (layout.scrollbar_track, layout.scrollbar_thumb) {
            let max_thumb_travel = (track.height - thumb.height).max(1.0);
            let dy = y - editor.scrollbar_drag_start_y;
            let max_scroll = (editor.content_height - layout.body.height).max(0.0);
            let scroll_ratio = dy / max_thumb_travel;
            editor.scroll_y = (editor.scrollbar_drag_start_scroll_y + scroll_ratio * max_scroll)
                .clamp(0.0, max_scroll);
            return true;
        }
    }

    // 4. 按钮悬停态更新
    let new_hovered = layout.hit_test_button(x, y);
    if editor.hovered_button != new_hovered {
        editor.hovered_button = new_hovered;
        return true;
    }

    false
}

/// 单次光标移动所需的坐标与屏幕尺寸。
#[derive(Debug, Clone, Copy)]
struct CursorFrame {
    x: f32,
    y: f32,
    screen_width: f32,
    screen_height: f32,
}

/// 空闲态更新窗口悬停与光标循环状态。
fn handle_idle_cursor(state: &mut SnipperState, frame: CursorFrame) -> bool {
    let hovered = window_at((frame.x, frame.y), &state.viewport.window_rects);
    let changed = hovered != state.viewport.hovered_window;
    state.viewport.hovered_window = hovered;
    if changed {
        state.viewport.window_cycle = None;
    }
    // 放大镜必须逐帧跟随光标；禁用放大镜时仍只在高亮窗口切换时重绘
    changed || state.viewport.magnifier.enabled
}

/// 创建态按当前约束更新选区终点。
fn handle_creating_cursor(
    state: &mut SnipperState,
    start: (f32, f32),
    _current: (f32, f32),
    frame: CursorFrame,
) -> bool {
    state.viewport.selection = SelectionState::Creating {
        start,
        current: state
            .viewport
            .selection_constraint
            .constrain(start, (frame.x, frame.y)),
    };
    true
}

/// 锁定态优先处理选区拖动、橡皮擦与绘制轨迹。
fn handle_locked_cursor(
    state: &mut SnipperState,
    rect: Rect,
    active_drag: Option<DragMode>,
    frame: CursorFrame,
) -> bool {
    if let Some(drag) = active_drag {
        update_selection_drag(state, rect, drag, frame);
        return true;
    }
    if state
        .editor
        .annotation
        .toolbar
        .contains_point(frame.x, frame.y)
    {
        return state
            .editor
            .annotation
            .toolbar
            .handle_mouse_move(frame.x, frame.y);
    }
    if state.editor.annotation.toolbar.active_tool == Some(Tool::Eraser) {
        let point = clamped_cursor_point(rect, frame);
        eraser::handle_drag_move(state, (point.x, point.y));
        return true;
    }
    if state.editor.annotation.toolbar.active_tool.is_some()
        && !state.editor.annotation.stroke.is_empty()
    {
        update_drawing_stroke(state, rect, frame);
        return true;
    }
    let hover_changed = state
        .editor
        .annotation
        .toolbar
        .handle_mouse_move(frame.x, frame.y);
    hover_changed || has_brush_cursor(state)
}

/// 更新选区移动或缩放手柄拖动，并同步工具栏布局。
fn update_selection_drag(state: &mut SnipperState, rect: Rect, drag: DragMode, frame: CursorFrame) {
    let (next_rect, next_drag) =
        crate::snipper::window::drag_calculator::calculate_drag(&rect, &drag, (frame.x, frame.y));
    state.viewport.selection = SelectionState::Locked {
        rect: next_rect,
        active_drag: Some(next_drag),
    };
    state.editor.annotation.toolbar.update_layout(
        &next_rect,
        frame.screen_width,
        frame.screen_height,
    );
}

/// 将光标夹取到锁定选区内并更新当前绘制工具轨迹。
fn update_drawing_stroke(state: &mut SnipperState, rect: Rect, frame: CursorFrame) {
    let point = clamped_cursor_point(rect, frame);
    let Some(tool) = state.editor.annotation.toolbar.active_tool else {
        return;
    };
    if tool == Tool::Pen && state.viewport.modifiers.shift_key() {
        constrain_pen_from_start(state, point);
        return;
    }
    if matches!(tool, Tool::Pen | Tool::Marker) {
        state.editor.annotation.stroke.points.push(point);
        return;
    }
    if tool == Tool::Mosaic
        && state
            .editor
            .annotation
            .toolbar
            .sub_options
            .mosaic_brush_mode
    {
        state.editor.annotation.stroke.append(point);
        return;
    }
    if tool == Tool::Line
        && state.editor.annotation.toolbar.sub_options.line_mode == LineMode::Polyline
    {
        // 折线由连续单击追加节点，鼠标移动只刷新预览，不修改已提交节点。
        return;
    }
    update_single_endpoint(state, point, tool);
}

/// Shift+画笔抬升为单向箭头约束。
fn constrain_pen_from_start(state: &mut SnipperState, point: crate::geometry::Point) {
    let Some(start) = state.editor.annotation.stroke.points.first().copied() else {
        return;
    };
    let endpoint = constrain_with_shift(Tool::Arrow, start, point);
    state.editor.annotation.stroke.points.truncate(1);
    state.editor.annotation.stroke.points.push(endpoint);
}

/// 两点式图元只保留起点与最新终点。
fn update_single_endpoint(state: &mut SnipperState, point: crate::geometry::Point, tool: Tool) {
    let endpoint = if state.viewport.modifiers.shift_key() {
        state
            .editor
            .annotation
            .stroke
            .points
            .first()
            .copied()
            .map_or(point, |start| constrain_with_shift(tool, start, point))
    } else {
        point
    };
    if state.editor.annotation.stroke.points.len() > 1 {
        state.editor.annotation.stroke.points[1] = endpoint;
    } else {
        state.editor.annotation.stroke.points.push(endpoint);
    }
}

/// 将光标坐标夹取到选区内，避免绘制落到选中区域之外。
fn clamped_cursor_point(rect: Rect, frame: CursorFrame) -> crate::geometry::Point {
    crate::geometry::Point::new(
        frame.x.clamp(rect.x, rect.x + rect.width),
        frame.y.clamp(rect.y, rect.y + rect.height),
    )
}

/// 按住 Shift 时的等比约束：矩形/椭圆取正方形，箭头吸附到 45° 的整数倍方向。
///
/// 起点保持不变，终点随约束移动；矩形取两次位移中较大的边长，保证拖拽方向与用户意图一致。
pub(super) fn constrain_with_shift(
    tool: Tool,
    start: crate::geometry::Point,
    current: crate::geometry::Point,
) -> crate::geometry::Point {
    let (x0, y0) = (start.x, start.y);
    let (mut x, mut y) = (current.x, current.y);
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

    crate::geometry::Point::new(x, y)
}

/// 当前工具是否需要持续跟随光标重绘圆形画笔轮廓。
fn has_brush_cursor(state: &SnipperState) -> bool {
    match state.editor.annotation.toolbar.active_tool {
        Some(Tool::Mosaic) => {
            state
                .editor
                .annotation
                .toolbar
                .sub_options
                .mosaic_brush_mode
        }
        Some(Tool::Eraser) => {
            !state
                .editor
                .annotation
                .toolbar
                .sub_options
                .eraser_rectangle_mode
        }
        _ => false,
    }
}
