//! 鼠标滚轮尺寸、字号与放大镜倍率调节。

use super::*;
use crate::editor::toolbar::{MosaicMode, SubControl};
use snapin_render::stitching::StitchDirection;

/// 处理截图会话中的鼠标滚轮事件 (连续动态调节当前标注与后续标注大小)
pub fn handle_mouse_wheel(state: &mut SnipperState, delta: MouseScrollDelta) -> bool {
    if let Some(editor) = state.ocr.editor.as_mut() {
        let Some(delta_y) = normalized_wheel_delta(delta) else {
            return false;
        };
        let scale = state.editor.annotation.toolbar.ui_scale;
        let body_height = 340.0 * scale;
        editor.scroll_by(-delta_y * 36.0, body_height);
        return true;
    }
    if state.ocr.menu.is_some() {
        return true;
    }
    let Some(delta_y) = normalized_wheel_delta(delta) else {
        return false;
    };

    if let Some(session) = state.long_capture.as_mut() {
        if session.is_editing {
            if let SelectionState::Locked { rect, .. } = state.viewport.selection {
                let (cx, cy) = (
                    state.viewport.cursor_pos.0 as f32,
                    state.viewport.cursor_pos.1 as f32,
                );
                if !state.editor.annotation.toolbar.contains_point(cx, cy) {
                    if !rect.contains(cx, cy) {
                        return false;
                    }
                    let viewport_span = match session.direction {
                        StitchDirection::Vertical => rect.height,
                        StitchDirection::Horizontal => rect.width,
                    };
                    session.scroll_viewport(delta_y * 48.0, viewport_span);
                    return true;
                }
            }
        } else {
            if session.is_capturing {
                // 捕获阶段不能在选区命中后直接返回。用户通常刚点击上方控制栏，光标仍停在
                // 选区外；只要长截图模式仍处于捕获态，任何滚轮事件都应交给事件分发器启动
                // 底层滚动抓屏，否则会出现“点击长截图后滚轮完全无反应”。
                // 反向自动裁剪统一交由拼接器在新帧位移确认后执行，避免重复裁剪。
                return true;
            }
            // 已暂停且未进入长图编辑框时，严禁滚轮穿透到普通截图的放大镜或隐藏工具栏控件。
            return false;
        }
    }

    if state.editor.annotation.toolbar.active_tool == Some(Tool::Magnifier)
        && !state.editor.annotation.toolbar.contains_point(
            state.viewport.cursor_pos.0 as f32,
            state.viewport.cursor_pos.1 as f32,
        )
        && adjust_magnifier_wheel(state, delta_y)
    {
        return true;
    }

    let selected_text_index = state
        .editor
        .selected_annotation
        .map(ShapeId::get)
        .filter(|&idx| {
            matches!(
                state.editor.annotations.shape_at(idx),
                Some(AnnotationShape::Text(_) | AnnotationShape::Translation(_))
            )
        });

    if state.editor.annotation.toolbar.handle_sub_wheel(
        state.viewport.cursor_pos.0 as f32,
        state.viewport.cursor_pos.1 as f32,
        delta_y,
    ) {
        if state.editor.annotation.toolbar.focused_sub_control() == Some(SubControl::CornerRadius) {
            state.editor.annotations.adjust_last_corner_radius(
                state.editor.annotation.toolbar.sub_options.corner_radius,
            );
        }
        if let Some(session) = &mut state.editor.annotation.active_text_input {
            session.font_size = state.editor.annotation.toolbar.sub_options.font_size;
        }
        if let Some(index) = selected_text_index {
            state
                .editor
                .annotations
                .adjust_font_size_at(index, state.editor.annotation.toolbar.sub_options.font_size);
        }
        if state.editor.annotation.toolbar.active_tool == Some(Tool::Magnifier) {
            sync_selected_magnifier_scale(state);
        }
        if let Some(control) = state.editor.annotation.toolbar.focused_sub_control() {
            crate::snipper::window::redaction::sync_redaction_style(state, control);
        }
        return true;
    }

    if state.editor.annotation.toolbar.active_tool == Some(Tool::Text)
        || state.editor.annotation.active_text_input.is_some()
        || selected_text_index.is_some()
    {
        adjust_text_wheel(state, delta_y, selected_text_index);
    } else {
        adjust_stroke_wheel(state, delta_y);
    }
    true
}

/// 将平台滚轮事件归一化为可处理增量，过滤零值、非有限值和抖动。
pub(crate) fn normalized_wheel_delta(delta: MouseScrollDelta) -> Option<f32> {
    let delta_y = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(position) => position.y as f32 / 20.0,
    };
    (delta_y.is_finite() && delta_y.abs() >= 0.1).then_some(delta_y)
}

/// 连续调节文字字号并同步当前文字会话与选中的文本图元。
fn adjust_text_wheel(state: &mut SnipperState, delta_y: f32, selected_text_index: Option<usize>) {
    let step = if delta_y > 0.0 { 2.0 } else { -2.0 };
    let new_font_size =
        (state.editor.annotation.toolbar.sub_options.font_size + step).clamp(12.0, 72.0);
    state.editor.annotation.toolbar.sub_options.font_size = new_font_size;
    if let Some(session) = &mut state.editor.annotation.active_text_input {
        session.font_size = new_font_size;
    }
    if let Some(index) = selected_text_index {
        state
            .editor
            .annotations
            .adjust_font_size_at(index, new_font_size);
    } else {
        state
            .editor
            .annotations
            .adjust_last_font_size(new_font_size);
    }
}

/// 连续调节描边粗细并同步尺寸等级与最近描边图元。
fn adjust_stroke_wheel(state: &mut SnipperState, delta_y: f32) {
    let step = if delta_y > 0.0 { 1.0 } else { -1.0 };
    let new_width = (state
        .editor
        .annotation
        .toolbar
        .sub_options
        .custom_stroke_width
        + step)
        .clamp(1.0, 48.0);
    state
        .editor
        .annotation
        .toolbar
        .sub_options
        .set_custom_stroke_width(new_width);
    let smart_redaction_active = state.editor.annotation.toolbar.active_tool == Some(Tool::Mosaic)
        && state.editor.annotation.toolbar.sub_options.mosaic_mode == MosaicMode::SmartRedact;
    if smart_redaction_active {
        crate::snipper::window::redaction::sync_redaction_style(state, SubControl::Size);
    } else {
        state.editor.annotations.adjust_last_stroke_width(new_width);
    }
}

/// 根据光标是否位于放大原区域或放大图像内，调整倍率或边框粗细。
fn adjust_magnifier_wheel(state: &mut SnipperState, delta_y: f32) -> bool {
    let index = selected_annotation_index(state);
    if cursor_over_magnifier(state, index) {
        let step = if delta_y > 0.0 { 0.25 } else { -0.25 };
        state.editor.annotation.toolbar.sub_options.magnifier_scale =
            (state.editor.annotation.toolbar.sub_options.magnifier_scale + step).clamp(1.0, 8.0);
        sync_selected_magnifier_scale(state);
    } else {
        adjust_magnifier_border(state, index, delta_y);
    }
    true
}

/// 返回当前选中的标注索引，未选中时回退到最近一条历史图元。
fn selected_annotation_index(state: &SnipperState) -> Option<usize> {
    state
        .editor
        .selected_annotation
        .map(ShapeId::get)
        .or_else(|| state.editor.annotations.undo_stack().len().checked_sub(1))
}

/// 判断光标是否位于放大镜源区域或目标区域内。
fn cursor_over_magnifier(state: &SnipperState, index: Option<usize>) -> bool {
    let Some(index) = index else {
        return false;
    };
    let Some(AnnotationShape::Magnifier(action)) = state.editor.annotations.shape_at(index) else {
        return false;
    };
    let point = (
        state.viewport.cursor_pos.0 as f32,
        state.viewport.cursor_pos.1 as f32,
    );
    action.source.contains(point.0, point.1) || action.dest.contains(point.0, point.1)
}

/// 调整放大镜边框粗细并同步当前选中图元。
fn adjust_magnifier_border(state: &mut SnipperState, index: Option<usize>, delta_y: f32) {
    let step = if delta_y > 0.0 { 1.0 } else { -1.0 };
    let width = state
        .editor
        .annotation
        .toolbar
        .sub_options
        .custom_stroke_width;
    let width = (width + step).clamp(1.0, 48.0);
    state
        .editor
        .annotation
        .toolbar
        .sub_options
        .custom_stroke_width = width;
    let Some(index) = index else {
        return;
    };
    let Some(mut shape) = state.editor.annotations.shape_at(index).cloned() else {
        return;
    };
    shape.adjust_size(snapin_annotation::shapes::SizeParam::Stroke(width));
    state.editor.annotations.replace(index, shape);
}

/// 将工具栏倍率同步到当前选中的放大镜图元。
fn sync_selected_magnifier_scale(state: &mut SnipperState) {
    let Some(index) = selected_annotation_index(state) else {
        return;
    };
    let Some(mut shape) = state.editor.annotations.shape_at(index).cloned() else {
        return;
    };
    let AnnotationShape::Magnifier(action) = &mut shape else {
        return;
    };
    let center = crate::geometry::Point::new(
        action.dest.x + action.dest.width / 2.0,
        action.dest.y + action.dest.height / 2.0,
    );
    action.adjust_scale(
        state.editor.annotation.toolbar.sub_options.magnifier_scale,
        Some(center),
    );
    state.editor.annotations.replace(index, shape);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalized_wheel_delta_rejects_invalid_values() {
        assert_eq!(
            normalized_wheel_delta(MouseScrollDelta::LineDelta(0.0, 1.0)),
            Some(1.0)
        );
        assert_eq!(
            normalized_wheel_delta(MouseScrollDelta::LineDelta(0.0, 0.0)),
            None
        );
        assert_eq!(
            normalized_wheel_delta(MouseScrollDelta::LineDelta(0.0, f32::NAN)),
            None
        );
        assert_eq!(
            normalized_wheel_delta(MouseScrollDelta::LineDelta(0.0, 0.05)),
            None
        );
    }
}
