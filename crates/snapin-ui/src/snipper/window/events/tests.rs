use super::*;
use crate::snipper::window::ocr_editor::OcrTextEditorState;
use winit::keyboard::ModifiersState;

/// 编辑框内 Enter 必须换行，Ctrl+Enter 才提交复制。
#[test]
fn test_ocr_editor_enter_and_ctrl_enter() {
    let mut state = SnipperState::new();
    state.ocr.editor = Some(OcrTextEditorState::new("第一行".to_string()));
    let image = RgbaImage::new(100, 100);

    assert!(handle_key_input(
        &mut state,
        &Key::Named(NamedKey::Enter),
        ElementState::Pressed,
        &image,
    ));
    assert_eq!(
        state
            .ocr
            .editor
            .as_ref()
            .map(OcrTextEditorState::finish_text)
            .as_deref(),
        Some("第一行\n")
    );

    state.viewport.modifiers = ModifiersState::CONTROL;
    assert!(handle_key_input(
        &mut state,
        &Key::Named(NamedKey::Enter),
        ElementState::Pressed,
        &image,
    ));
    assert!(state.has_pending_clipboard());
}

/// 编辑框内 Ctrl+A 全选并能直接替换输入。
#[test]
fn test_ocr_editor_navigation_and_shortcuts() {
    let mut state = SnipperState::new();
    state.ocr.editor = Some(OcrTextEditorState::new("原始文字".to_string()));
    let image = RgbaImage::new(100, 100);

    // Ctrl+A 全选
    state.viewport.modifiers = ModifiersState::CONTROL;
    assert!(handle_key_input(
        &mut state,
        &Key::Character(winit::keyboard::SmolStr::new("a")),
        ElementState::Pressed,
        &image,
    ));
    assert!(state
        .ocr
        .editor
        .as_ref()
        .unwrap()
        .selection_range()
        .is_some());

    // 输入替换
    state.viewport.modifiers = ModifiersState::empty();
    assert!(handle_key_input(
        &mut state,
        &Key::Character(winit::keyboard::SmolStr::new("新")),
        ElementState::Pressed,
        &image,
    ));
    assert_eq!(state.ocr.editor.as_ref().unwrap().finish_text(), "新");
}

#[test]
fn test_long_capture_escape_closes_crop_submenu_then_aborts_cleanly() {
    use crate::snipper::selection::{Rect, SelectionState};
    use crate::snipper::window::state::{LongCaptureSession, SnipperPhase};
    use crate::snipper::window::types::SnipperOutcome;

    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 10.0,
            y: 10.0,
            width: 100.0,
            height: 100.0,
        },
        active_drag: None,
    };
    let mut session = LongCaptureSession::new(RgbaImage::new(100, 100));
    session.crop_sub_menu_open = true;
    state.long_capture = Some(session);
    assert!(state.jobs.begin_screen_capture().is_some());
    state.set_status_message("长截图进行中");

    let image = RgbaImage::new(200, 200);
    // 第一次按 Esc：先收起裁剪子菜单，不销毁长截图会话
    assert!(handle_key_input(
        &mut state,
        &Key::Named(NamedKey::Escape),
        ElementState::Pressed,
        &image,
    ));
    assert!(state.long_capture.is_some());
    assert!(!state.long_capture.as_ref().unwrap().crop_sub_menu_open);
    assert_eq!(state.phase(), SnipperPhase::LongCapturing);

    // 第二次按 Esc：彻底终止长截图、取消在途抓屏请求并将选区退回 Idle，不得残留幽灵状态
    assert!(handle_key_input(
        &mut state,
        &Key::Named(NamedKey::Escape),
        ElementState::Pressed,
        &image,
    ));
    assert!(state.long_capture.is_none());
    assert!(matches!(state.viewport.selection, SelectionState::Idle));
    assert_eq!(state.phase(), SnipperPhase::Idle);
    assert!(!state.jobs.has_pending_screen_capture());
    assert!(state.session.outcome.is_none());

    // 第三次按 Esc：确认退出整个截图会话
    assert!(handle_key_input(
        &mut state,
        &Key::Named(NamedKey::Escape),
        ElementState::Pressed,
        &image,
    ));
    assert!(matches!(
        state.session.outcome,
        Some(SnipperOutcome::Cancelled)
    ));
}

#[test]
fn test_long_capture_editing_escape_steps_and_right_click_cleanup() {
    use crate::editor::text_input::TextInputSession;
    use crate::geometry::Point;
    use crate::model::ShapeId;
    use crate::snipper::selection::{Rect, SelectionState};
    use crate::snipper::window::mouse::handle_mouse_button;
    use crate::snipper::window::state::{LongCaptureSession, SnipperPhase};
    use snapin_annotation::shapes::RectAction;
    use snapin_annotation::LineStyle;
    use winit::event::MouseButton;

    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 20.0,
            y: 20.0,
            width: 120.0,
            height: 120.0,
        },
        active_drag: None,
    };
    let mut session = LongCaptureSession::new(RgbaImage::new(120, 120));
    session.enter_editing();
    state.long_capture = Some(session);

    let shape = AnnotationShape::Rect(RectAction {
        rect: snapin_annotation::Rect::new(10.0, 10.0, 30.0, 30.0),
        color: (255, 0, 0, 255).into(),
        stroke_width: 2.0,
        is_filled: false,
        corner_radius: 0.0,
        line_style: LineStyle::Solid,
    });
    state.editor.annotations.push(shape);
    state.editor.selected_annotation = Some(ShapeId::from(0));
    state.editor.annotation.toolbar.active_tool = Some(Tool::Rect);
    state.editor.annotation.stroke.begin(Point::new(10.0, 10.0));
    state.editor.annotation.active_text_input =
        Some(TextInputSession::new((30.0, 30.0), (255, 0, 0, 255), 16.0));

    let image = RgbaImage::new(200, 200);
    // 1. 取消文本输入（按键盘 Esc，验证委托给 handle_escape 且保留编辑态）
    assert!(handle_key_input(
        &mut state,
        &Key::Named(NamedKey::Escape),
        ElementState::Pressed,
        &image,
    ));
    assert!(state.editor.annotation.active_text_input.is_none());
    assert_eq!(state.phase(), SnipperPhase::LongCaptureEditing);

    // 2. 取消在绘草稿笔迹
    assert!(handle_escape(&mut state));
    assert!(!state.editor.annotation.stroke.is_drawing);
    assert_eq!(state.phase(), SnipperPhase::LongCaptureEditing);

    // 3. 取消激活的工具并清除选中态
    assert!(handle_escape(&mut state));
    assert!(state.editor.annotation.toolbar.active_tool.is_none());
    assert!(state.editor.selected_annotation.is_none());
    assert_eq!(state.phase(), SnipperPhase::LongCaptureEditing);

    // 3.5 无激活工具时仅取消标注选中态，仍保留编辑态
    state.editor.selected_annotation = Some(ShapeId::from(0));
    assert!(handle_escape(&mut state));
    assert!(state.editor.selected_annotation.is_none());
    assert_eq!(state.phase(), SnipperPhase::LongCaptureEditing);

    // 4. 右键退场：清理长截图会话、长图期标注历史并将选区置回 Idle
    assert!(handle_mouse_button(
        &mut state,
        MouseButton::Right,
        ElementState::Pressed,
        &image,
    ));
    assert!(state.long_capture.is_none());
    assert!(matches!(state.viewport.selection, SelectionState::Idle));
    assert_eq!(state.phase(), SnipperPhase::Idle);
    assert!(!state.editor.annotations.can_undo());
    assert!(state.session.outcome.is_none());
}

#[test]
fn test_long_capture_toolbar_close_and_text_reedit_escape() {
    use crate::editor::text_input::TextInputSession;
    use crate::editor::toolbar::ToolbarAction;
    use crate::snipper::selection::{Rect, SelectionState};
    use crate::snipper::window::mouse::toolbar_action::dispatch_toolbar_action;
    use crate::snipper::window::state::{LongCaptureSession, SnipperPhase};
    use crate::snipper::window::types::SnipperOutcome;

    let mut state = SnipperState::new();
    state.viewport.selection = SelectionState::Locked {
        rect: Rect {
            x: 10.0,
            y: 10.0,
            width: 120.0,
            height: 120.0,
        },
        active_drag: None,
    };
    let mut session = LongCaptureSession::new(RgbaImage::new(120, 120));
    session.enter_editing();
    state.long_capture = Some(session);
    assert!(state.jobs.begin_screen_capture().is_some());

    let mut original_builder = TextInputSession::new((15.0, 20.0), (255, 0, 0, 255), 16.0);
    original_builder.push_str("保留文本");
    let original_text = original_builder.commit().expect("应成功生成文本图元");
    let mut text_session = TextInputSession::new((15.0, 20.0), (255, 0, 0, 255), 16.0);
    text_session.original_shape = Some(original_text);
    state.editor.annotation.active_text_input = Some(text_session);

    let image = RgbaImage::new(200, 200);
    // 键盘 Esc 放弃二次文本编辑时必须恢复原图元并留在长图编辑态
    assert!(handle_key_input(
        &mut state,
        &Key::Named(NamedKey::Escape),
        ElementState::Pressed,
        &image,
    ));
    assert!(state.editor.annotation.active_text_input.is_none());
    assert!(state.editor.annotations.can_undo());
    assert_eq!(state.phase(), SnipperPhase::LongCaptureEditing);

    // 点击编辑工具栏 Close 必须清理长截图会话、待决抓屏与长图标注，并取消会话
    assert!(dispatch_toolbar_action(
        &mut state,
        ToolbarAction::Close,
        &image,
    ));
    assert!(state.long_capture.is_none());
    assert!(matches!(state.viewport.selection, SelectionState::Idle));
    assert!(!state.jobs.has_pending_screen_capture());
    assert!(!state.editor.annotations.can_undo());
    assert!(matches!(
        state.session.outcome,
        Some(SnipperOutcome::Cancelled)
    ));
}
