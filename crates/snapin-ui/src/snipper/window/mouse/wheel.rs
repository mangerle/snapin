//! 鼠标滚轮尺寸、字号与放大镜倍率调节。

use super::*;

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

    if state.toolbar.active_tool == Some(Tool::Text) || state.active_text_input.is_some() {
        adjust_text_wheel(state, delta_y);
    } else {
        adjust_stroke_wheel(state, delta_y);
    }
    true
}

/// 连续调节文字字号并同步当前文字会话与最近文本图元。
fn adjust_text_wheel(state: &mut SnipperState, delta_y: f32) {
    let step = if delta_y > 0.0 { 2.0 } else { -2.0 };
    let new_font_size = (state.toolbar.sub_options.font_size + step).clamp(12.0, 72.0);
    state.toolbar.sub_options.font_size = new_font_size;
    if let Some(session) = &mut state.active_text_input {
        session.font_size = new_font_size;
    }
    state.annotations.adjust_last_font_size(new_font_size);
}

/// 连续调节描边粗细并同步尺寸等级与最近描边图元。
fn adjust_stroke_wheel(state: &mut SnipperState, delta_y: f32) {
    let step = if delta_y > 0.0 { 1.0 } else { -1.0 };
    let new_width = (state.toolbar.sub_options.custom_stroke_width + step).clamp(1.0, 48.0);
    state.toolbar.sub_options.custom_stroke_width = new_width;
    state.toolbar.sub_options.stroke_width = if new_width <= 2.5 {
        StrokeWidthLevel::Thin
    } else if new_width <= 5.5 {
        StrokeWidthLevel::Medium
    } else {
        StrokeWidthLevel::Thick
    };
    state.annotations.adjust_last_stroke_width(new_width);
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
                shape.adjust_size(snapin_annotation::shapes::SizeParam::Stroke(
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
