//! 已提交标注的二次编辑：命中选中、整体拖动与包围盒手柄拉伸。
//!
//! 交互约定：默认只有文档最新一条标注可被选中编辑，历史标注处于锁定态，
//! 让绘制工具可以继续在旧标注上方作画；按住 Alt 可显式解锁任意历史标注。

use super::drag_calculator::calculate_drag;
use super::events::SnipperState;
use crate::editor::text_input::TextInputSession;
use crate::editor::toolbar::Tool;
use crate::model::ShapeId;
use crate::snipper::selection::{hit_test_handle, DragMode, Rect, ResizeHandle};
use snapin_annotation::AnnotationShape;

/// 将指定索引的文本标注还原为活跃编辑会话，支持就地二次修改。
pub fn start_editing_text_annotation(state: &mut SnipperState, index: usize) -> bool {
    let is_text = matches!(
        state.editor.annotations.shape_at(index),
        Some(AnnotationShape::Text(_))
    );
    if !is_text {
        return false;
    }
    let Some(AnnotationShape::Text(action)) = state.editor.annotations.take_at(index) else {
        return false;
    };

    state.editor.annotation.toolbar.active_tool = Some(Tool::Text);
    let mut opts = state.editor.annotation.toolbar.sub_options;
    let (r, g, b, a) = action.color.to_tuple();
    opts.color = tiny_skia::Color::from_rgba8(r, g, b, a);
    opts.text_bold = action.bold;
    opts.text_italic = action.italic;
    opts.text_background_enabled = action.background_color.is_some();
    state.editor.annotation.toolbar.sub_options = opts;

    let session = TextInputSession::from_text_action(action);
    state.editor.annotation.active_text_input = Some(session);
    clear(state);
    true
}

/// 标注编辑拖拽会话：始终基于按下瞬间的原始图元重建，避免逐帧累积误差
#[derive(Debug, Clone)]
pub enum AnnotationDrag {
    /// 整体移动
    Move {
        /// 按下位置，用于计算总位移
        origin: (f32, f32),
        original: AnnotationShape,
    },
    /// 包围盒手柄拉伸
    Resize {
        handle: ResizeHandle,
        /// 拖拽开始时的包围盒
        original_bounds: Rect,
        original: AnnotationShape,
    },
    /// 拖动放大镜原区域，同时移动原区域与放大图像
    MagnifierSource {
        origin: (f32, f32),
        original: AnnotationShape,
    },
    /// 仅拖动放大镜图像
    MagnifierImage {
        origin: (f32, f32),
        original: AnnotationShape,
    },
}

/// 已选中标注的包围盒
pub fn selected_bounds(state: &SnipperState) -> Option<Rect> {
    let index = state.editor.selected_annotation?;
    state.editor.annotations.bounds_at(index.get())
}

/// 已选中标注是否支持手柄拉伸
pub fn selected_supports_resize(state: &SnipperState) -> bool {
    state
        .editor
        .selected_annotation
        .and_then(|index| state.editor.annotations.shape_at(index.get()))
        .is_some_and(AnnotationShape::supports_resize)
}

/// 命中已选中标注的缩放手柄
pub fn hit_test_selected_handle(state: &SnipperState, cx: f32, cy: f32) -> Option<ResizeHandle> {
    if !selected_supports_resize(state) {
        return None;
    }
    let rect = selected_bounds(state)?;
    hit_test_handle(
        &rect,
        cx,
        cy,
        8.0 * state.editor.annotation.toolbar.ui_scale,
    )
}

/// 命中当前允许编辑的标注：默认只允许最新一条，`allow_locked`（按住 Alt）时允许任意一条。
///
/// 设计原理：绘制工具激活时，历史图元若整块包围盒都可点击，用户几乎无法在其上方继续绘制。
/// 把默认可编辑范围收敛到最新一条，既保留"画完就能立刻微调"的体验，又让旧标注重新变成
/// 可绘制的画布；需要修改旧标注时用 Alt 显式解锁，避免误拖。
pub fn hit_test_editable(
    state: &SnipperState,
    point: (f32, f32),
    allow_locked: bool,
) -> Option<usize> {
    if allow_locked {
        return state.editor.annotations.hit_test(point);
    }
    let latest = state.editor.annotations.undo_stack().len().checked_sub(1)?;
    let bounds = state.editor.annotations.bounds_at(latest)?;
    bounds.contains(point.0, point.1).then_some(latest)
}

/// 点击位置是否命中了被锁定的历史标注（非最新一条，且未按 Alt 解锁）。
///
/// 用于给出"按住 Alt 可编辑"的提示，并避免无绘制工具时点击旧标注被误判为移动选区。
pub fn hit_test_locked(state: &SnipperState, point: (f32, f32)) -> bool {
    let Some(index) = state.editor.annotations.hit_test(point) else {
        return false;
    };
    hit_test_editable(state, point, false) != Some(index)
}

/// 点击是否落在当前选中标注的本体上；用于继续拖动已选中的旧标注。
pub fn hit_test_selected_body(state: &SnipperState, cx: f32, cy: f32) -> bool {
    selected_bounds(state).is_some_and(|rect| rect.contains(cx, cy))
}

/// 把选中项切换到当前最新图元；文档为空时清除选中。
///
/// 新画完、撤销与重做之后都调用它，保证"当前可编辑的标注"始终是文档最后一条。
pub fn select_latest(state: &mut SnipperState) {
    state.editor.annotation_drag = None;
    let latest = state.editor.annotations.undo_stack().len().checked_sub(1);
    state.editor.selected_annotation = latest.map(ShapeId::from);
    if let Some(index) = latest {
        if let Some(AnnotationShape::Text(action)) = state.editor.annotations.shape_at(index) {
            state.editor.annotation.toolbar.sub_options.font_size = action.font_size;
            let (r, g, b, a) = action.color.to_tuple();
            state.editor.annotation.toolbar.sub_options.color =
                tiny_skia::Color::from_rgba8(r, g, b, a);
        }
    }
}

/// 处理左键按下：优先命中已选中标注的手柄，其次命中当前允许编辑的标注并开始整体拖动。
///
/// 默认可编辑范围只有最新一条图元；按住 Alt 可显式选中任意历史图元。已经选中的标注
/// （包括通过 Alt 选中的旧图元）在后续按下时仍可直接拖动，否则每次微调都要重复按 Alt。
/// 返回 true 表示事件已被标注编辑消费；点击不可编辑区域返回 false 并取消当前选中，
/// 让选区移动、重新框选与继续绘制逻辑执行。
pub fn handle_press(state: &mut SnipperState, cx: f32, cy: f32, allow_locked: bool) -> bool {
    if try_start_handle_resize(state, cx, cy)
        || try_start_magnifier_drag(state, cx, cy)
        || try_start_selected_move(state, cx, cy)
    {
        return true;
    }

    if let Some(index) = hit_test_editable(state, (cx, cy), allow_locked) {
        state.editor.selected_annotation = Some(index.into());
        if let Some(original) = state.editor.annotations.shape_at(index).cloned() {
            if let AnnotationShape::Text(action) = &original {
                state.editor.annotation.toolbar.sub_options.font_size = action.font_size;
                let (r, g, b, a) = action.color.to_tuple();
                state.editor.annotation.toolbar.sub_options.color =
                    tiny_skia::Color::from_rgba8(r, g, b, a);
            }
            state.editor.annotation_drag = Some(AnnotationDrag::Move {
                origin: (cx, cy),
                original,
            });
        }
        return true;
    }
    clear(state);
    false
}

/// 开始通过包围盒手柄拉伸已选标注。
fn try_start_handle_resize(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    let Some(handle) = hit_test_selected_handle(state, cx, cy) else {
        return false;
    };
    let Some(index) = state.editor.selected_annotation else {
        return false;
    };
    let Some(original_bounds) = selected_bounds(state) else {
        return false;
    };
    let Some(original) = state.editor.annotations.shape_at(index.get()).cloned() else {
        return false;
    };
    state.editor.annotation_drag = Some(AnnotationDrag::Resize {
        handle,
        original_bounds,
        original,
    });
    true
}

/// 放大镜的源区域和放大图可分别拖动。
fn try_start_magnifier_drag(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    let Some(index) = state.editor.selected_annotation else {
        return false;
    };
    let Some(original) = state.editor.annotations.shape_at(index.get()).cloned() else {
        return false;
    };
    let AnnotationShape::Magnifier(action) = &original else {
        return false;
    };
    let source = action.source;
    let dest = action.dest;
    if source.contains(cx, cy) {
        state.editor.annotation_drag = Some(AnnotationDrag::MagnifierSource {
            origin: (cx, cy),
            original,
        });
        return true;
    }
    if dest.contains(cx, cy) {
        state.editor.annotation_drag = Some(AnnotationDrag::MagnifierImage {
            origin: (cx, cy),
            original,
        });
        return true;
    }
    false
}

/// 已选中的标注允许继续整体拖动，不要求重复按住 Alt。
fn try_start_selected_move(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    if !hit_test_selected_body(state, cx, cy) {
        return false;
    }
    let Some(index) = state.editor.selected_annotation else {
        return false;
    };
    let Some(original) = state.editor.annotations.shape_at(index.get()).cloned() else {
        return false;
    };
    state.editor.annotation_drag = Some(AnnotationDrag::Move {
        origin: (cx, cy),
        original,
    });
    true
}

/// 处理标注拖拽过程中的鼠标移动；返回 true 表示画面已变化需要重绘。
pub fn handle_drag_move(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    let Some(index) = state.editor.selected_annotation else {
        state.editor.annotation_drag = None;
        return false;
    };
    let Some(drag) = state.editor.annotation_drag.as_ref() else {
        return false;
    };

    let updated = match drag {
        AnnotationDrag::Move { origin, original } => {
            let mut shape = original.clone();
            shape.translate(cx - origin.0, cy - origin.1);
            shape
        }
        AnnotationDrag::Resize {
            handle,
            original_bounds,
            original,
        } => {
            let (new_bounds, _) =
                calculate_drag(original_bounds, &DragMode::Resize(*handle), (cx, cy));
            let mut shape = original.clone();
            shape.resize_from_bounds(*original_bounds, new_bounds);
            shape
        }
        AnnotationDrag::MagnifierSource { origin, original } => {
            let mut shape = original.clone();
            if let AnnotationShape::Magnifier(action) = &mut shape {
                action.translate_source(cx - origin.0, cy - origin.1);
            }
            shape
        }
        AnnotationDrag::MagnifierImage { origin, original } => {
            let mut shape = original.clone();
            if let AnnotationShape::Magnifier(action) = &mut shape {
                action.translate_dest(cx - origin.0, cy - origin.1);
            }
            shape
        }
    };

    if state.editor.annotations.replace(index.get(), updated) {
        true
    } else {
        clear(state);
        false
    }
}

/// 结束标注拖拽；返回 true 表示确实存在进行中的拖拽
pub fn handle_drag_release(state: &mut SnipperState) -> bool {
    state.editor.annotation_drag.take().is_some()
}

/// 清除标注选中与拖拽状态
pub fn clear(state: &mut SnipperState) {
    state.editor.selected_annotation = None;
    state.editor.annotation_drag = None;
}

#[cfg(test)]
mod tests;
