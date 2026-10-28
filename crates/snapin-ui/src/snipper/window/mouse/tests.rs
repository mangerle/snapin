//! 鼠标交互单元测试用例。

use super::super::types::SnipMode;
use super::*;
use snapin_core::annotation::action::AnnotationShape;
use snapin_core::annotation::shapes::RectAction;
use snapin_core::annotation::LineStyle;

/// 空闲态鼠标移动必须触发重绘，保证放大镜实时跟随光标
#[test]
fn test_idle_cursor_move_requests_redraw_for_magnifier() {
    let mut state = SnipperState::new();
    state.magnifier.enabled = true;

    assert!(
        handle_cursor_move(&mut state, (100.0, 100.0), 1920.0, 1080.0),
        "首次移动必须请求重绘以显示放大镜"
    );
    assert!(state.cursor_ready);
    assert_eq!(state.cursor_pos, (100.0, 100.0));

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
    state.magnifier.enabled = false;

    assert!(!handle_cursor_move(
        &mut state,
        (100.0, 100.0),
        1920.0,
        1080.0
    ));
    assert!(state.cursor_ready);
}

#[test]
fn test_mouse_wheel_continuous_stroke_width_and_retroactive_adjust() {
    let mut state = SnipperState::new();
    // 初始粗细为 4.0
    assert_eq!(state.toolbar.sub_options.custom_stroke_width, 4.0);

    // 模拟已绘制一个矩形标注
    state.annotations.push(AnnotationShape::Rect(RectAction {
        rect: (10.0, 10.0, 100.0, 100.0),
        color: (255, 0, 0, 255),
        stroke_width: 4.0,
        is_filled: false,
        corner_radius: 0.0,
        line_style: LineStyle::Solid,
    }));

    // 滚轮向上滚动：4.0 -> 5.0
    let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
    let changed = handle_mouse_wheel(&mut state, delta);
    assert!(changed);
    assert_eq!(state.toolbar.sub_options.custom_stroke_width, 5.0);

    // 再次向上：5.0 -> 6.0，档位变为 Thick
    let changed = handle_mouse_wheel(&mut state, delta);
    assert!(changed);
    assert_eq!(state.toolbar.sub_options.custom_stroke_width, 6.0);
    assert_eq!(
        state.toolbar.sub_options.stroke_width,
        StrokeWidthLevel::Thick
    );
}

#[test]
fn test_mouse_wheel_text_mode_font_size_adjust() {
    let mut state = SnipperState::new();
    state.toolbar.active_tool = Some(ToolbarItem::ToolText);
    let initial_sz = state.toolbar.sub_options.font_size;

    // 处于文字模式，向上滚字号增大 2.0
    let delta = MouseScrollDelta::LineDelta(0.0, 1.0);
    let changed = handle_mouse_wheel(&mut state, delta);
    assert!(changed);
    assert_eq!(state.toolbar.sub_options.font_size, initial_sz + 2.0);
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
    state.selection = SelectionState::Locked {
        rect: selection,
        active_drag: None,
    };
    // 激活序号印章工具
    state.toolbar.active_tool = Some(ToolbarItem::ToolBadge);
    state.toolbar.update_layout(&selection, 1920.0, 1080.0);

    let original = RgbaImage::new(1920, 1080);

    // 模拟点击二级属性栏中的颜色位置（位于 sub_rect 内）
    let sub_rect = state.toolbar.sub_rect.expect("序号印章应具备二级属性栏");
    let click_x = sub_rect.x + 10.0;
    let click_y = sub_rect.y + 10.0;

    assert!(state.toolbar.contains_point(click_x, click_y));

    let pressed = handle_left_press(&mut state, click_x, click_y, &original);
    assert!(pressed, "工具栏点击应被有效拦截消费");
    assert!(state.pending_stroke_points.is_empty(), "不应产生待定笔触点");

    let released = handle_left_release(&mut state, click_x, click_y, &original);
    assert!(released, "释放事件应被安全消费");

    // 核心断言：绝不应该生成任何标注，序号数字也不应该自增
    assert!(
        !state.annotations.can_undo(),
        "工具栏点击绝不应生成画布标注"
    );
    assert_eq!(
        state.annotations.next_badge_number(),
        1,
        "序号印章不应被误创建"
    );
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
    state.selection = SelectionState::Locked {
        rect: selection,
        active_drag: None,
    };
    // 确保未激活特定标注工具
    state.toolbar.active_tool = None;

    let original = RgbaImage::new(1920, 1080);

    // 1. 点击右下角手柄位置 (300.0, 250.0)
    let pressed = handle_left_press(&mut state, 300.0, 250.0, &original);
    assert!(pressed);

    match &state.selection {
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
    if let SelectionState::Locked { rect, active_drag } = &mut state.selection {
        if let Some(drag) = active_drag {
            let (r, d) =
                crate::snipper::window::drag_calculator::calculate_drag(rect, drag, (cx, cy));
            *rect = r;
            *active_drag = Some(d);
        }
    }

    match &state.selection {
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
        constrain_with_shift(ToolbarItem::ToolRect, (10.0, 10.0), (110.0, 50.0)),
        (110.0, 110.0)
    );
    // 反向拖拽同样成立
    assert_eq!(
        constrain_with_shift(ToolbarItem::ToolEllipse, (110.0, 110.0), (10.0, 50.0)),
        (10.0, 10.0)
    );
    // 纯水平拖拽时纵向位移为 0，正负号无定义，约定向正方向展开成正方形
    assert_eq!(
        constrain_with_shift(ToolbarItem::ToolRect, (0.0, 0.0), (80.0, 0.0)),
        (80.0, 80.0)
    );

    // 箭头：接近水平的角度吸附到 0°
    let (ax, ay) = constrain_with_shift(ToolbarItem::ToolArrow, (0.0, 0.0), (100.0, 8.0));
    assert!(ay.abs() < 0.01, "接近水平的箭头应吸附为水平，实际 y={ay}");
    assert!(ax > 99.0 && ax < 102.0, "长度应保持不变，实际 x={ax}");

    // 箭头：约 42° 吸附到 45°，两个分量应基本相等
    let (bx, by) = constrain_with_shift(ToolbarItem::ToolArrow, (0.0, 0.0), (100.0, 90.0));
    assert!((bx - by).abs() < 0.5, "应吸附到 45°，实际 ({bx}, {by})");

    // 非几何工具不受约束
    assert_eq!(
        constrain_with_shift(ToolbarItem::ToolPen, (0.0, 0.0), (10.0, 30.0)),
        (10.0, 30.0)
    );
}

/// 截图翻译模式：首次锁定选区后应立即派发翻译任务，并复位模式避免重复触发。
///
/// 测试环境未注入后台任务调度器，因此以「调度器不可用」提示作为翻译派发已发生的证据。
#[test]
fn test_translate_mode_triggers_translation_on_first_selection() {
    let mut state = SnipperState::new();
    state.mode = SnipMode::Translate;
    state.selection = SelectionState::Creating {
        start: (10.0, 10.0),
        current: (110.0, 110.0),
    };
    let original = RgbaImage::new(200, 200);

    assert!(handle_left_release(&mut state, 110.0, 110.0, &original));
    assert!(
        matches!(state.selection, SelectionState::Locked { .. }),
        "有效拖拽后选区应锁定"
    );
    assert_eq!(state.mode, SnipMode::Normal, "自动翻译应复位为普通模式");
    assert_eq!(
        state.status_message.as_deref(),
        Some("后台任务调度器不可用"),
        "应进入翻译任务派发路径"
    );
}

/// 普通模式框选后不得自动触发翻译，避免影响常规截图流程
#[test]
fn test_normal_mode_does_not_auto_translate() {
    let mut state = SnipperState::new();
    state.selection = SelectionState::Creating {
        start: (10.0, 10.0),
        current: (110.0, 110.0),
    };
    let original = RgbaImage::new(200, 200);

    assert!(handle_left_release(&mut state, 110.0, 110.0, &original));
    assert!(
        state.status_message.is_none(),
        "普通模式不应产生翻译派发提示"
    );
}
