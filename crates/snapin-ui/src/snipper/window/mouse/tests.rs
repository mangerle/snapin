//! 鼠标交互单元测试用例。

use super::super::types::SnipMode;
use super::*;
use crate::editor::toolbar::{
    MosaicMode, StrokeWidthLevel, SubControl, ToolbarAction, ToolbarItem,
};
use crate::geometry::Point;
use crate::ports::OcrTextItem;
use crate::snipper::window::ocr_menu::OcrMenuAction;
use crate::snipper::window::redaction::{apply_redaction, is_redaction_applied, RedactionPreview};
use snapin_annotation::action::AnnotationShape;
use snapin_annotation::shapes::{MagnifierAction, MagnifierConnector, MagnifierShape, RectAction};
use snapin_annotation::ColorRgba;
use snapin_annotation::LineStyle;

/// 空闲态鼠标移动必须触发重绘，保证放大镜实时跟随光标
#[test]
fn test_idle_cursor_move_requests_redraw_for_magnifier() {
    let mut state = SnipperState::new();
    state.viewport.magnifier.enabled = true;

    assert!(
        handle_cursor_move(&mut state, (100.0, 100.0), 1920.0, 1080.0),
        "首次移动必须请求重绘以显示放大镜"
    );
    assert!(state.viewport.cursor_ready);
    assert_eq!(state.viewport.cursor_pos, (100.0, 100.0));

    // 悬停区域未变化时仍要逐帧重绘，否则放大镜会停在旧位置
    assert!(
        handle_cursor_move(&mut state, (120.0, 80.0), 1920.0, 1080.0),
        "光标移动必须持续请求重绘"
    );
}

/// 放大镜禁用时空闲态移动无需重绘，避免无意义的整屏刷新
#[test]
fn test_idle_cursor_move_skips_redraw_without_magnifier() {
    let mut state = SnipperState::new();
    state.viewport.magnifier.enabled = false;

    assert!(!handle_cursor_move(
        &mut state,
        (100.0, 100.0),
        1920.0,
        1080.0
    ));
    assert!(state.viewport.cursor_ready);
}

/// 橡皮擦激活时鼠标移到二级栏仍必须更新控件悬停，保证悬浮提示正常出现。
#[test]
fn test_eraser_sub_bar_hover_updates_tooltip_target() {
    let mut state = SnipperState::new();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 500.0,
        height: 300.0,
    };
    state.viewport.selection = SelectionState::Locked {
        rect: selection,
        active_drag: None,
    };
    state.editor.annotation.toolbar.active_tool = Some(Tool::Eraser);
    state
        .editor
        .annotation
        .toolbar
        .update_layout(&selection, 1920.0, 1080.0);
    let brush = state
        .editor
        .annotation
        .toolbar
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::EraserBrush))
        .expect("橡皮擦二级栏应包含画笔模式");

    assert!(handle_cursor_move(
        &mut state,
        (
            (brush.x + brush.width / 2.0) as f64,
            (brush.y + brush.height / 2.0) as f64,
        ),
        1920.0,
        1080.0,
    ));
    assert_eq!(
        state.editor.annotation.toolbar.hovered_sub_control,
        Some(SubControl::EraserBrush)
    );
}

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

#[test]
fn test_toolbar_click_does_not_create_annotation() {
    let mut state = SnipperState::new();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 800.0,
        height: 600.0,
    };
    state.viewport.selection = SelectionState::Locked {
        rect: selection,
        active_drag: None,
    };
    // 激活序号印章工具
    state.editor.annotation.toolbar.active_tool = Some(Tool::Badge);
    state
        .editor
        .annotation
        .toolbar
        .update_layout(&selection, 1920.0, 1080.0);

    let original = RgbaImage::new(1920, 1080);

    // 模拟点击二级属性栏中的颜色位置（位于 sub_rect 内）
    let sub_rect = state
        .editor
        .annotation
        .toolbar
        .sub_rect
        .expect("序号印章应具备二级属性栏");
    let click_x = sub_rect.x + 10.0;
    let click_y = sub_rect.y + 10.0;

    assert!(state
        .editor
        .annotation
        .toolbar
        .contains_point(click_x, click_y));

    let pressed = handle_left_press(&mut state, click_x, click_y, &original);
    assert!(pressed, "工具栏点击应被有效拦截消费");
    assert!(
        state.editor.annotation.stroke.points.is_empty(),
        "不应产生待定笔触点"
    );

    let released = handle_left_release(&mut state, click_x, click_y, &original);
    assert!(released, "释放事件应被安全消费");

    // 核心断言：绝不应该生成任何标注，序号数字也不应该自增
    assert!(
        !state.editor.annotations.can_undo(),
        "工具栏点击绝不应生成画布标注"
    );
    assert_eq!(
        state.editor.annotations.next_badge_number(),
        1,
        "序号印章不应被误创建"
    );
}

/// 右键文字识别按钮必须打开“复制文字/编辑文字”菜单，点击菜单项后关闭菜单。
#[test]
fn test_right_click_ocr_button_opens_context_menu() {
    let mut state = SnipperState::new();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 800.0,
        height: 600.0,
    };
    state.viewport.selection = SelectionState::Locked {
        rect: selection,
        active_drag: None,
    };
    state
        .editor
        .annotation
        .toolbar
        .update_layout(&selection, 1920.0, 1080.0);
    let ocr_item = ToolbarItem::Action(ToolbarAction::Ocr);
    let ocr_rect = state
        .editor
        .annotation
        .toolbar
        .main_item_rect(ocr_item)
        .expect("主栏应包含文字识别按钮");
    state.viewport.cursor_pos = (
        (ocr_rect.x + ocr_rect.width / 2.0) as f64,
        (ocr_rect.y + ocr_rect.height / 2.0) as f64,
    );
    let original = RgbaImage::new(1920, 1080);

    assert!(handle_mouse_button(
        &mut state,
        MouseButton::Right,
        ElementState::Pressed,
        &original
    ));
    let edit_rect = state
        .ocr
        .menu
        .as_ref()
        .and_then(|menu| menu.row_rect(OcrMenuAction::EditText))
        .expect("右键菜单应包含编辑文字");
    state.viewport.cursor_pos = (
        (edit_rect.x + edit_rect.width / 2.0) as f64,
        (edit_rect.y + edit_rect.height / 2.0) as f64,
    );
    assert!(handle_mouse_button(
        &mut state,
        MouseButton::Left,
        ElementState::Pressed,
        &original
    ));
    assert!(state.ocr.menu.is_some(), "按下菜单项时不得提前关闭菜单");
    assert!(handle_mouse_button(
        &mut state,
        MouseButton::Left,
        ElementState::Released,
        &original
    ));
    assert!(state.ocr.menu.is_none(), "点击菜单项后应关闭菜单");
}

/// 清空全部标注动作必须一次性移除当前会话中的全部图元。
#[test]
fn test_clear_annotations_action_removes_all_shapes() {
    let mut state = SnipperState::new();
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
    let original = RgbaImage::new(800, 600);

    assert!(super::toolbar_action::dispatch_toolbar_action(
        &mut state,
        ToolbarAction::ClearAnnotations,
        &original
    ));
    assert!(state.editor.annotations.undo_stack().is_empty());
}

/// 序号工具单击画布直接放置序号图元，不得误开文字输入会话。
#[test]
fn test_badge_click_places_shape_without_text_session() {
    let mut state = SnipperState::new();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 400.0,
        height: 300.0,
    };
    state.viewport.selection = SelectionState::Locked {
        rect: selection,
        active_drag: None,
    };
    state.editor.annotation.toolbar.active_tool = Some(Tool::Badge);
    state
        .editor
        .annotation
        .toolbar
        .update_layout(&selection, 800.0, 600.0);
    let original = RgbaImage::new(800, 600);

    assert!(handle_left_press(&mut state, 220.0, 200.0, &original));
    assert!(
        state.editor.annotation.active_text_input.is_none(),
        "序号不得开启文字输入"
    );
    assert!(handle_left_release(&mut state, 220.0, 200.0, &original));

    assert!(matches!(
        state.editor.annotations.undo_stack().last(),
        Some(AnnotationShape::Badge(_))
    ));
    assert_eq!(state.editor.annotations.next_badge_number(), 2);
}

/// 智能打码完成后点击文字区域应实时增删对应马赛克，无需回车确认。
#[test]
fn test_redaction_region_click_toggles_mosaic_immediately() {
    let mut state = SnipperState::new();
    let preview = RedactionPreview::from_ocr_lines(&[OcrTextItem {
        text: "敏感文字".to_string(),
        bounds: snapin_foundation::Rect::new(10.0, 20.0, 80.0, 24.0),
    }]);
    apply_redaction(&mut state, &preview);
    let rect = preview.regions[0];
    state.ocr.redaction_preview = Some(preview);
    state.viewport.cursor_pos = (20.0, 25.0);
    let original = RgbaImage::new(800, 600);

    assert!(handle_mouse_button(
        &mut state,
        MouseButton::Left,
        ElementState::Pressed,
        &original
    ));
    assert!(!is_redaction_applied(&state.editor.annotations, rect));
    assert!(handle_mouse_button(
        &mut state,
        MouseButton::Left,
        ElementState::Released,
        &original
    ));

    assert!(handle_mouse_button(
        &mut state,
        MouseButton::Left,
        ElementState::Pressed,
        &original
    ));
    assert!(is_redaction_applied(&state.editor.annotations, rect));
}

/// 智能打码滚轮调节必须同步本次 OCR 命中的全部马赛克，而不是只改最后一条。
#[test]
fn test_mouse_wheel_smart_redaction_updates_all_masks() {
    let mut state = SnipperState::new();
    state.editor.annotation.toolbar.active_tool = Some(Tool::Mosaic);
    state.editor.annotation.toolbar.sub_options.mosaic_mode = MosaicMode::SmartRedact;
    state.editor.annotation.toolbar.sub_options.effect_strength = 60;
    let preview = RedactionPreview::from_ocr_lines(&[
        OcrTextItem {
            text: "第一行敏感文字".to_string(),
            bounds: snapin_foundation::Rect::new(10.0, 20.0, 80.0, 24.0),
        },
        OcrTextItem {
            text: "第二行敏感文字".to_string(),
            bounds: snapin_foundation::Rect::new(10.0, 60.0, 80.0, 24.0),
        },
    ]);
    apply_redaction(&mut state, &preview);
    state.ocr.redaction_preview = Some(preview.clone());

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
        8.5
    );
    assert_eq!(
        state.editor.annotation.toolbar.sub_options.effect_strength,
        68
    );
    for rect in &preview.regions {
        let index = state
            .editor
            .annotations
            .undo_stack()
            .iter()
            .rposition(|shape| match shape {
                AnnotationShape::Mosaic(action) => {
                    action.rect.x == rect.x
                        && action.rect.y == rect.y
                        && action.rect.width == rect.width
                        && action.rect.height == rect.height
                }
                _ => false,
            })
            .expect("每个 OCR 区域都应有对应马赛克");
        let Some(AnnotationShape::Mosaic(action)) = state.editor.annotations.shape_at(index) else {
            panic!("目标图元必须是马赛克");
        };
        assert_eq!(action.cell_size, 17, "全部智能打码必须使用同一颗粒度");
    }
}

#[test]
fn test_hit_test_resize_handle_starts_resizing() {
    let mut state = SnipperState::new();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 200.0,
        height: 150.0,
    };
    state.viewport.selection = SelectionState::Locked {
        rect: selection,
        active_drag: None,
    };
    // 确保未激活特定标注工具
    state.editor.annotation.toolbar.active_tool = None;

    let original = RgbaImage::new(1920, 1080);

    // 1. 点击右下角手柄位置 (300.0, 250.0)
    let pressed = handle_left_press(&mut state, 300.0, 250.0, &original);
    assert!(pressed);

    match &state.viewport.selection {
        SelectionState::Locked { active_drag, .. } => {
            assert_eq!(
                *active_drag,
                Some(DragMode::Resize(
                    crate::snipper::selection::ResizeHandle::BottomRight
                )),
                "点击右下角控制手柄应激活 Resize(BottomRight) 拖拽模式"
            );
        }
        _ => panic!("预期处于 Locked 状态"),
    }

    // 2. 移动鼠标拉大选区至 (350.0, 300.0)
    // 构造一个虚拟 Window
    // handle_cursor_move 会直接调用 calculate_drag
    let (cx, cy) = (350.0f32, 300.0f32);
    if let SelectionState::Locked { rect, active_drag } = &mut state.viewport.selection {
        if let Some(drag) = active_drag {
            let (r, d) =
                crate::snipper::window::drag_calculator::calculate_drag(rect, drag, (cx, cy));
            *rect = r;
            *active_drag = Some(d);
        }
    }

    match &state.viewport.selection {
        SelectionState::Locked { rect, .. } => {
            assert_eq!(rect.width, 250.0);
            assert_eq!(rect.height, 200.0);
        }
        _ => panic!("预期处于 Locked 状态"),
    }
}

/// Shift 等比约束：矩形/椭圆取正方形，箭头吸附 45° 整数倍
#[test]
fn test_shift_constraint_shapes_and_arrow() {
    // 矩形：取较长边构成正方形，方向保持不变
    assert_eq!(
        constrain_with_shift(Tool::Rect, Point::new(10.0, 10.0), Point::new(110.0, 50.0)),
        Point::new(110.0, 110.0)
    );
    // 反向拖拽同样成立
    assert_eq!(
        constrain_with_shift(
            Tool::Ellipse,
            Point::new(110.0, 110.0),
            Point::new(10.0, 50.0)
        ),
        Point::new(10.0, 10.0)
    );
    // 纯水平拖拽时纵向位移为 0，正负号无定义，约定向正方向展开成正方形
    assert_eq!(
        constrain_with_shift(Tool::Rect, Point::new(0.0, 0.0), Point::new(80.0, 0.0)),
        Point::new(80.0, 80.0)
    );

    // 箭头：接近水平的角度吸附到 0°
    let arrow = constrain_with_shift(Tool::Arrow, Point::new(0.0, 0.0), Point::new(100.0, 8.0));
    assert!(
        arrow.y.abs() < 0.01,
        "接近水平的箭头应吸附为水平，实际 y={}",
        arrow.y
    );
    assert!(
        arrow.x > 99.0 && arrow.x < 102.0,
        "长度应保持不变，实际 x={}",
        arrow.x
    );

    // 箭头：约 42° 吸附到 45°，两个分量应基本相等
    let diagonal = constrain_with_shift(Tool::Arrow, Point::new(0.0, 0.0), Point::new(100.0, 90.0));
    assert!(
        (diagonal.x - diagonal.y).abs() < 0.5,
        "应吸附到 45°，实际 ({}, {})",
        diagonal.x,
        diagonal.y
    );

    // 非几何工具不受约束
    assert_eq!(
        constrain_with_shift(Tool::Pen, Point::new(0.0, 0.0), Point::new(10.0, 30.0)),
        Point::new(10.0, 30.0)
    );
}

/// 截图翻译模式：首次锁定选区后应立即派发翻译任务，并复位模式避免重复触发。
///
/// 测试环境未注入后台媒体执行器，因此以「媒体执行器不可用」提示作为翻译派发已发生的证据。
#[test]
fn test_translate_mode_triggers_translation_on_first_selection() {
    let mut state = SnipperState::new();
    state.session.mode = SnipMode::Translate;
    state.viewport.selection = SelectionState::Creating {
        start: (10.0, 10.0),
        current: (110.0, 110.0),
    };
    let original = RgbaImage::new(200, 200);

    assert!(handle_left_release(&mut state, 110.0, 110.0, &original));
    assert!(
        matches!(state.viewport.selection, SelectionState::Locked { .. }),
        "有效拖拽后选区应锁定"
    );
    assert_eq!(
        state.session.mode,
        SnipMode::Normal,
        "自动翻译应复位为普通模式"
    );
    assert_eq!(
        state.session.status_message.as_deref(),
        Some("后台任务媒体执行器不可用"),
        "应进入翻译任务派发路径"
    );
}

/// 普通模式框选后不得自动触发翻译，避免影响常规截图流程
#[test]
fn test_normal_mode_does_not_auto_translate() {
    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Creating {
        start: (10.0, 10.0),
        current: (110.0, 110.0),
    };
    let original = RgbaImage::new(200, 200);

    assert!(handle_left_release(&mut state, 110.0, 110.0, &original));
    assert!(
        state.session.status_message.is_none(),
        "普通模式不应产生翻译派发提示"
    );
}

/// 画完一个标注后应立刻选中、可直接拖动；画第二个后第一个回到锁定态，
/// 点击旧标注变成继续绘制画布，按住 Alt 才能重新进入编辑。
#[test]
fn test_new_annotation_is_selected_and_old_ones_lock() {
    use super::super::annotation_edit::{clear, AnnotationDrag};

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
    state.editor.annotation.toolbar.active_tool = Some(Tool::Rect);
    let original = RgbaImage::new(800, 600);

    // 画第一个矩形：松手后应立刻选中，可直接进入拖动
    assert!(handle_left_press(&mut state, 100.0, 100.0, &original));
    handle_cursor_move(&mut state, (200.0, 180.0), 800.0, 600.0);
    assert!(handle_left_release(&mut state, 200.0, 180.0, &original));
    assert_eq!(
        state.editor.selected_annotation,
        Some(0.into()),
        "新标注应自动选中"
    );
    assert!(handle_left_press(&mut state, 150.0, 140.0, &original));
    assert!(
        matches!(
            state.editor.annotation_drag,
            Some(AnnotationDrag::Move { .. })
        ),
        "选中后可立即拖动"
    );
    handle_left_release(&mut state, 150.0, 140.0, &original);

    // 画第二个矩形：新标注接管可编辑权
    assert!(handle_left_press(&mut state, 400.0, 300.0, &original));
    handle_cursor_move(&mut state, (500.0, 380.0), 800.0, 600.0);
    assert!(handle_left_release(&mut state, 500.0, 380.0, &original));
    assert_eq!(state.editor.selected_annotation, Some(1.into()));

    // 取消选中后点击旧标注：不再进入编辑，而是作为画布继续绘制新图元
    clear(&mut state);
    assert!(handle_left_press(&mut state, 150.0, 140.0, &original));
    assert!(state.editor.selected_annotation.is_none(), "旧标注默认锁定");
    assert!(state.editor.annotation_drag.is_none());
    state.editor.annotation.stroke.points.clear();

    // 未激活绘制工具时点击旧标注只给出解锁提示，不误拖动选区
    state.editor.annotation.toolbar.active_tool = None;
    assert!(handle_left_press(&mut state, 150.0, 140.0, &original));
    assert!(state.editor.selected_annotation.is_none());
    assert!(state
        .session
        .status_message
        .as_deref()
        .is_some_and(|message| message.contains("Alt")));

    // 按住 Alt 才可显式解锁旧标注
    state.viewport.modifiers = winit::keyboard::ModifiersState::ALT;
    assert!(handle_left_press(&mut state, 150.0, 140.0, &original));
    assert_eq!(state.editor.selected_annotation, Some(0.into()));
    assert!(matches!(
        state.editor.annotation_drag,
        Some(AnnotationDrag::Move { .. })
    ));
}
