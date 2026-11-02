//! 橡皮擦工具的框选与画笔擦除交互。

use super::SnipperState;
use crate::geometry::Rect;

/// 开始橡皮擦操作；画笔模式先擦除起点，框选模式记录拖拽起点。
pub(super) fn handle_press(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    if state.editor.toolbar.sub_options.eraser_rectangle_mode {
        state.editor.stroke.begin((cx, cy));
    } else {
        erase_brush_point(state, (cx, cy));
    }
    true
}

/// 拖拽期间更新框选范围，或在画笔模式下持续擦除命中标注。
pub(super) fn handle_drag_move(state: &mut SnipperState, point: (f32, f32)) -> bool {
    if state.editor.toolbar.sub_options.eraser_rectangle_mode {
        state.editor.stroke.update_endpoint(point);
    } else {
        erase_brush_point(state, point);
    }
    true
}

/// 松开鼠标：框选模式擦除与范围相交的标注，画笔模式消费最后一笔。
pub(super) fn handle_release(state: &mut SnipperState, cx: f32, cy: f32) -> bool {
    if state.editor.toolbar.sub_options.eraser_rectangle_mode {
        let start = state
            .editor
            .stroke
            .points
            .first()
            .copied()
            .unwrap_or((cx, cy));
        let erase_rect = Rect::from_points(start, (cx, cy));
        state.editor.annotations.erase_intersecting(erase_rect);
        state.editor.stroke.clear();
        super::super::annotation_edit::clear(state);
    } else {
        erase_brush_point(state, (cx, cy));
    }
    true
}

/// 擦除画笔半径内命中的标注。当前实现按图元包围盒相交判定。
fn erase_brush_point(state: &mut SnipperState, point: (f32, f32)) {
    let radius = state
        .editor
        .toolbar
        .sub_options
        .custom_stroke_width
        .max(4.0);
    let rect = Rect {
        x: point.0 - radius,
        y: point.1 - radius,
        width: radius * 2.0,
        height: radius * 2.0,
    };
    state.editor.annotations.erase_intersecting(rect);
}
