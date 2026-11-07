
use super::*;
use crate::editor::toolbar::Tool;
use crate::snipper::selection::SelectionState;
use snapin_annotation::shapes::{MagnifierAction, MagnifierConnector, MagnifierShape, RectAction};
use snapin_annotation::ColorRgba;
use snapin_annotation::LineStyle;

fn state_with_rect() -> SnipperState {
    let mut state = SnipperState::new();
    state.editor.annotation.toolbar.ui_scale = 1.0;
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        },
        active_drag: None,
    };
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(100.0, 100.0, 200.0, 120.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
    state
}

fn state_with_magnifier() -> SnipperState {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 300.0,
        },
        active_drag: None,
    };
    state
        .editor
        .annotations
        .push(AnnotationShape::Magnifier(MagnifierAction {
            source: Rect::new(10.0, 10.0, 20.0, 20.0),
            dest: Rect::new(100.0, 100.0, 40.0, 40.0),
            shape: MagnifierShape::Rectangle,
            border_width: 2.0,
            border_color: ColorRgba::opaque(255, 255, 255),
            smooth: true,
            shadow: true,
            connector: MagnifierConnector::Solid,
        }));
    state.editor.selected_annotation = Some(0.into());
    state
}

/// 拖动原区域时，原区域和放大图像同步移动。
#[test]
fn test_magnifier_source_drag_moves_both_regions() {
    let mut state = state_with_magnifier();
    assert!(handle_press(&mut state, 20.0, 20.0, false));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::MagnifierSource { .. })
    ));
    assert!(handle_drag_move(&mut state, 30.0, 40.0));
    match state.editor.annotations.shape_at(0) {
        Some(AnnotationShape::Magnifier(action)) => {
            assert_eq!(action.source, Rect::new(20.0, 30.0, 20.0, 20.0));
            assert_eq!(action.dest, Rect::new(110.0, 120.0, 40.0, 40.0));
        }
        other => panic!("应为放大镜图元，实际: {other:?}"),
    }
}

/// 拖动放大图像区域时只移动图像，原区域保持不动。
#[test]
fn test_magnifier_image_drag_moves_only_output() {
    let mut state = state_with_magnifier();
    assert!(handle_press(&mut state, 110.0, 110.0, false));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::MagnifierImage { .. })
    ));
    assert!(handle_drag_move(&mut state, 115.0, 117.0));
    match state.editor.annotations.shape_at(0) {
        Some(AnnotationShape::Magnifier(action)) => {
            assert_eq!(action.source, Rect::new(10.0, 10.0, 20.0, 20.0));
            assert_eq!(action.dest, Rect::new(105.0, 107.0, 40.0, 40.0));
        }
        other => panic!("应为放大镜图元，实际: {other:?}"),
    }
}

/// 点击标注本体应立即选中并进入整体拖动，位移只取决于按下点到当前光标的总差
#[test]
fn test_press_and_drag_moves_annotation() {
    let mut state = state_with_rect();

    assert!(handle_press(&mut state, 150.0, 150.0, false));
    assert_eq!(state.editor.selected_annotation, Some(0.into()));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::Move { .. })
    ));

    assert!(handle_drag_move(&mut state, 180.0, 130.0));
    let bounds = state
        .editor
        .annotations
        .bounds_at(0)
        .expect("矩形必须有包围盒");
    assert_eq!(bounds.x, 130.0);
    assert_eq!(bounds.y, 80.0);

    assert!(handle_drag_release(&mut state));
    assert!(state.editor.annotation_drag.is_none());
}

/// 已选中标注的手柄应进入拉伸模式，并按原始包围盒映射尺寸
#[test]
fn test_resize_handle_scales_annotation() {
    let mut state = state_with_rect();
    assert!(handle_press(&mut state, 150.0, 150.0, false));

    // 右下角手柄位于 (300, 220)，向外拖到 (400, 320)
    assert!(handle_press(&mut state, 300.0, 220.0, false));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::Resize { .. })
    ));

    assert!(handle_drag_move(&mut state, 400.0, 320.0));
    let bounds = state
        .editor
        .annotations
        .bounds_at(0)
        .expect("矩形必须有包围盒");
    assert_eq!(bounds.x, 100.0);
    assert_eq!(bounds.y, 100.0);
    assert_eq!(bounds.width, 300.0);
    assert_eq!(bounds.height, 220.0);
}

/// 点击空白取消选中，且不消费事件（交还给选区逻辑）
#[test]
fn test_press_blank_clears_selection() {
    let mut state = state_with_rect();
    assert!(handle_press(&mut state, 150.0, 150.0, false));
    assert!(!handle_press(&mut state, 600.0, 500.0, false));
    assert!(state.editor.selected_annotation.is_none());
    assert!(state.editor.annotation_drag.is_none());
}

/// 标注工具激活时仍应允许直接点击已有标注进入编辑，而不是强制先取消工具
#[test]
fn test_active_tool_still_allows_annotation_editing() {
    let mut state = state_with_rect();
    state.editor.annotation.toolbar.active_tool = Some(Tool::Pen);
    assert!(handle_press(&mut state, 150.0, 150.0, false));
    assert_eq!(state.editor.selected_annotation, Some(0.into()));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::Move { .. })
    ));
}

/// 构造两个互不重叠的矩形，索引 1 为最新一条。
fn state_with_two_rects() -> SnipperState {
    let mut state = state_with_rect();
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(300.0, 300.0, 100.0, 100.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 2.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
    state
}

/// 默认可编辑范围只有最新一条；旧标注必须按住 Alt 才能解锁。
#[test]
fn test_only_latest_annotation_is_editable_by_default() {
    let mut state = state_with_two_rects();

    // 最新的矩形可直接选中并进入拖动
    assert!(handle_press(&mut state, 350.0, 350.0, false));
    assert_eq!(state.editor.selected_annotation, Some(1.into()));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::Move { .. })
    ));

    // 旧矩形默认锁定：不消费事件，也不留下选中状态
    clear(&mut state);
    assert!(!handle_press(&mut state, 150.0, 150.0, false));
    assert!(state.editor.selected_annotation.is_none());

    // 按住 Alt 才能显式解锁旧标注
    assert!(handle_press(&mut state, 150.0, 150.0, true));
    assert_eq!(state.editor.selected_annotation, Some(0.into()));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::Move { .. })
    ));
}

/// Alt 选中的旧标注在后续按下时无需重复按住 Alt 即可继续拖动。
#[test]
fn test_selected_old_annotation_stays_draggable() {
    let mut state = state_with_two_rects();
    assert!(handle_press(&mut state, 150.0, 150.0, true));
    assert!(handle_drag_release(&mut state));

    assert!(handle_press(&mut state, 150.0, 150.0, false));
    assert_eq!(state.editor.selected_annotation, Some(0.into()));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::Move { .. })
    ));
}

/// 历史变化后选中应重新落到文档最后一条；空文档则清除选中。
#[test]
fn test_select_latest_follows_document_tail() {
    let mut state = state_with_two_rects();
    select_latest(&mut state);
    assert_eq!(state.editor.selected_annotation, Some(1.into()));

    state.editor.annotations.clear();
    select_latest(&mut state);
    assert!(state.editor.selected_annotation.is_none());
}
