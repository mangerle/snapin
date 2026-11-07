
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
