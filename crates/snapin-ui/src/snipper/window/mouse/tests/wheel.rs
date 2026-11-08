//! 鼠标滚轮交互测试。

use super::super::*;
use crate::editor::toolbar::items::StrokeWidthLevel;
use crate::editor::toolbar::SubControl;
use crate::snipper::window::state::LongCaptureSession;
use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::{MagnifierAction, MagnifierConnector, MagnifierShape, RectAction};
use snapin_annotation::{ColorRgba, LineStyle};

#[test]
fn test_mouse_wheel_continuous_stroke_width_and_retroactive_adjust() {
    let mut state = SnipperState::new();
    // 初始粗细为 4.0
    assert_eq!(
        state
            .editor
            .annotation
            .toolbar
            .sub_options
            .custom_stroke_width,
        4.0
    );

    // 模拟已绘制一个矩形标注
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(10.0, 10.0, 100.0, 100.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 4.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));

    // 滚轮向上滚动：4.0 -> 5.0
    let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
    let changed = handle_mouse_wheel(&mut state, delta);
    assert!(changed);
    assert_eq!(
        state
            .editor
            .annotation
            .toolbar
            .sub_options
            .custom_stroke_width,
        5.0
    );

    // 再次向上：5.0 -> 6.0，档位变为 Thick
    let changed = handle_mouse_wheel(&mut state, delta);
    assert!(changed);
    assert_eq!(
        state
            .editor
            .annotation
            .toolbar
            .sub_options
            .custom_stroke_width,
        6.0
    );
    assert_eq!(
        state.editor.annotation.toolbar.sub_options.stroke_width,
        StrokeWidthLevel::Thick
    );
}

#[test]
fn test_mouse_wheel_text_mode_font_size_adjust() {
    let mut state = SnipperState::new();
    state.editor.annotation.toolbar.active_tool = Some(Tool::Text);
    let initial_sz = state.editor.annotation.toolbar.sub_options.font_size;

    // 处于文字模式，向上滚字号增大 2.0
    let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
    let changed = handle_mouse_wheel(&mut state, delta);
    assert!(changed);
    assert_eq!(
        state.editor.annotation.toolbar.sub_options.font_size,
        initial_sz + 2.0
    );
}

/// 圆角控件滚轮必须同时更新后续参数和当前已绘制的矩形图元。
#[test]
fn test_corner_radius_wheel_updates_current_rectangle() {
    let mut state = SnipperState::new();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 600.0,
        height: 400.0,
    };
    state.viewport.selection = SelectionState::Locked {
        rect: selection,
        active_drag: None,
    };
    state.editor.annotation.toolbar.active_tool = Some(Tool::Rect);
    state
        .editor
        .annotation
        .toolbar
        .update_layout(&selection, 1920.0, 1080.0);
    state
        .editor
        .annotations
        .push(AnnotationShape::Rect(RectAction {
            rect: Rect::new(150.0, 150.0, 240.0, 120.0),
            color: (255, 0, 0, 255).into(),
            stroke_width: 4.0,
            is_filled: false,
            corner_radius: 0.0,
            line_style: LineStyle::Solid,
        }));
    let corner = state
        .editor
        .annotation
        .toolbar
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::CornerRadius))
        .expect("矩形二级栏应包含圆角控件");
    state.viewport.cursor_pos = (
        (corner.x + corner.width / 2.0) as f64,
        (corner.y + corner.height / 2.0) as f64,
    );

    assert!(handle_mouse_wheel(
        &mut state,
        MouseScrollDelta::LineDelta(0.0, 1.0)
    ));
    assert_eq!(
        state.editor.annotation.toolbar.sub_options.corner_radius,
        2.0
    );
    let Some(AnnotationShape::Rect(action)) = state.editor.annotations.shape_at(0) else {
        panic!("当前图元必须是矩形");
    };
    assert_eq!(action.corner_radius, 2.0);
}

fn state_with_magnifier_wheel_target() -> SnipperState {
    let mut state = SnipperState::new();
    state.editor.annotation.toolbar.active_tool = Some(Tool::Magnifier);
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 500.0,
            height: 400.0,
        },
        active_drag: None,
    };
    state.editor.annotation.toolbar.update_layout(
        &Rect {
            x: 0.0,
            y: 0.0,
            width: 500.0,
            height: 400.0,
        },
        1000.0,
        800.0,
    );
    state
        .editor
        .annotations
        .push(AnnotationShape::Magnifier(MagnifierAction {
            source: Rect::new(10.0, 10.0, 40.0, 40.0),
            dest: Rect::new(100.0, 100.0, 80.0, 80.0),
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

/// 在放大图范围内滚动滚轮只调整倍率，并保持目标区域中心不变。
#[test]
fn test_magnifier_wheel_over_image_adjusts_scale() {
    let mut state = state_with_magnifier_wheel_target();
    state.viewport.cursor_pos = (140.0, 140.0);

    assert!(handle_mouse_wheel(
        &mut state,
        MouseScrollDelta::LineDelta(0.0, 1.0)
    ));

    assert_eq!(
        state.editor.annotation.toolbar.sub_options.magnifier_scale,
        2.25
    );
    match state.editor.annotations.shape_at(0) {
        Some(AnnotationShape::Magnifier(action)) => {
            assert_eq!(action.source, Rect::new(10.0, 10.0, 40.0, 40.0));
            assert_eq!(action.dest, Rect::new(95.0, 95.0, 90.0, 90.0));
        }
        other => panic!("应为放大镜图元，实际: {other:?}"),
    }
}

/// 在源区域和放大图之外滚动滚轮只调整边框粗细。
#[test]
fn test_magnifier_wheel_outside_image_adjusts_border_width() {
    let mut state = state_with_magnifier_wheel_target();
    state.viewport.cursor_pos = (400.0, 350.0);

    assert!(handle_mouse_wheel(
        &mut state,
        MouseScrollDelta::LineDelta(0.0, 1.0)
    ));

    assert_eq!(
        state
            .editor
            .annotation
            .toolbar
            .sub_options
            .custom_stroke_width,
        5.0
    );
    match state.editor.annotations.shape_at(0) {
        Some(AnnotationShape::Magnifier(action)) => {
            assert_eq!(action.border_width, 5.0);
            assert_eq!(action.dest, Rect::new(100.0, 100.0, 80.0, 80.0));
        }
        other => panic!("应为放大镜图元，实际: {other:?}"),
    }
}

/// 长截图录制只响应当前选区内部的滚轮，避免隐藏覆盖层后在选区外误滚动。
#[test]
fn test_long_capture_wheel_requires_cursor_inside_selection() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 300.0,
        },
        active_drag: None,
    };
    state.long_capture = Some(LongCaptureSession::new(RgbaImage::new(400, 300)));

    state.viewport.cursor_pos = (50.0, 50.0);
    assert!(!handle_mouse_wheel(
        &mut state,
        MouseScrollDelta::LineDelta(0.0, -1.0)
    ));

    state.viewport.cursor_pos = (200.0, 200.0);
    assert!(handle_mouse_wheel(
        &mut state,
        MouseScrollDelta::LineDelta(0.0, -1.0)
    ));
}

/// 反向滚轮不在事件阶段提前裁剪，等新帧位移确认后由拼接器统一执行。
#[test]
fn test_long_capture_reverse_wheel_does_not_double_crop() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 300.0,
        },
        active_drag: None,
    };
    state.long_capture = Some(LongCaptureSession::new(RgbaImage::new(400, 300)));
    state.viewport.cursor_pos = (200.0, 200.0);

    assert!(handle_mouse_wheel(
        &mut state,
        MouseScrollDelta::LineDelta(0.0, 1.0)
    ));
    assert_eq!(
        state
            .long_capture
            .as_ref()
            .map(|session| session.stitcher.canvas().height()),
        Some(300)
    );
}

/// 选中已提交的文字标注后，滚轮必须能够直接调节该文字标注的字号。
#[test]
fn test_mouse_wheel_adjusts_font_size_of_selected_text_annotation() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        },
        active_drag: None,
    };
    let original = image::RgbaImage::new(800, 600);

    // 1. 提交一条字号为 16.0 的文字标注
    let mut session =
        crate::editor::text_input::TextInputSession::new((100.0, 100.0), (255, 0, 0, 255), 16.0);
    session.push_str("测试字号调整");
    state
        .editor
        .annotations
        .push(session.commit().expect("应生成 TextAction"));
    assert_eq!(state.editor.annotations.undo_stack().len(), 1);

    // 2. 清除当前活跃工具，模拟选择模式
    state.editor.annotation.toolbar.active_tool = None;

    // 3. 点击选中该文字标注
    assert!(super::super::buttons::handle_left_press(
        &mut state, 110.0, 105.0, &original
    ));
    assert!(super::super::buttons::handle_left_release(
        &mut state, 110.0, 105.0, &original
    ));
    assert!(state.editor.selected_annotation.is_some());

    // 4. 向上滚动滚轮，字号增大 2.0 -> 18.0
    let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
    assert!(handle_mouse_wheel(&mut state, delta));
    assert_eq!(state.editor.annotation.toolbar.sub_options.font_size, 18.0);
    match state.editor.annotations.shape_at(0) {
        Some(AnnotationShape::Text(action)) => {
            assert_eq!(action.font_size, 18.0, "选中文本图元字号应随滚轮增大");
        }
        other => panic!("应为 Text 图元，实际: {other:?}"),
    }

    // 5. 向下滚动滚轮，字号减小 2.0 -> 16.0
    let delta_down = MouseScrollDelta::LineDelta(0.0, -1.0);
    assert!(handle_mouse_wheel(&mut state, delta_down));
    assert_eq!(state.editor.annotation.toolbar.sub_options.font_size, 16.0);
    match state.editor.annotations.shape_at(0) {
        Some(AnnotationShape::Text(action)) => {
            assert_eq!(action.font_size, 16.0, "选中文本图元字号应随滚轮减小");
        }
        other => panic!("应为 Text 图元，实际: {other:?}"),
    }
}
