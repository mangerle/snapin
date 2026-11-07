//! OCR 全文编辑框单元测试：编辑、选区、排版整理、统计与渲染。

use super::*;
use tiny_skia::{Color, Pixmap};

#[test]
fn test_ocr_editor_insert_and_delete() {
    let mut editor = OcrTextEditorState::new("Hello World".to_string());
    assert_eq!(editor.cursor(), 11);

    // 光标左移 5 次到 "World" 前
    for _ in 0..5 {
        editor.move_left(false);
    }
    assert_eq!(editor.cursor(), 6);

    // 插入 "Beautiful "
    editor.insert_str("Beautiful ");
    assert_eq!(editor.text(), "Hello Beautiful World");

    // 退格删除一个字符
    editor.pop_char_backward();
    assert_eq!(editor.text(), "Hello BeautifulWorld");
}

#[test]
fn test_ocr_editor_selection_and_replacement() {
    let mut editor = OcrTextEditorState::new("测试文字一二三".to_string());
    editor.select_all();
    assert_eq!(editor.selection_range(), Some((0, "测试文字一二三".len())));
    assert_eq!(editor.selected_text().as_deref(), Some("测试文字一二三"));

    // 选区输入替换
    editor.insert_str("全新内容");
    assert_eq!(editor.text(), "全新内容");
    assert_eq!(editor.selection_range(), None);
}

#[test]
fn test_ocr_editor_home_and_end() {
    let mut editor = OcrTextEditorState::new("第一行文本\n第二行内容".to_string());
    editor.move_doc_start(false);
    assert_eq!(editor.cursor(), 0);

    editor.move_end(false);
    assert_eq!(editor.cursor(), "第一行文本".len());

    editor.move_doc_end(false);
    assert_eq!(editor.cursor(), editor.text().len());
}

#[test]
fn test_ocr_editor_format_layout() {
    let mut editor = OcrTextEditorState::new(
        "   第一段文本   \n\n\n\n   第二段文本  \n\n   第三段文本   ".to_string(),
    );
    editor.format_layout();
    assert_eq!(editor.text(), "第一段文本\n\n第二段文本\n\n第三段文本");
}

#[test]
fn test_ocr_editor_counts() {
    let editor = OcrTextEditorState::new("一二三四五\n六七八九十".to_string());
    assert_eq!(editor.char_count(), 11); // 10 个中文字符 + 1 个换行
    assert_eq!(editor.line_count(), 2);
}

#[test]
fn test_ocr_editor_scroll_and_clamp() {
    let mut editor = OcrTextEditorState::new("长文本".to_string());
    editor.content_height = 500.0;
    let body_height = 300.0;

    editor.scroll_by(50.0, body_height);
    assert_eq!(editor.scroll_y, 50.0);

    // 超出最大滚动应夹取在 200.0
    editor.scroll_by(300.0, body_height);
    assert_eq!(editor.scroll_y, 200.0);

    // 向上滚动负数应夹取在 0.0
    editor.scroll_by(-500.0, body_height);
    assert_eq!(editor.scroll_y, 0.0);
}

#[test]
fn test_ocr_editor_layout_hit_test() {
    let editor = OcrTextEditorState::new("测试内容".to_string());
    let layout = editor.layout(1000.0, 800.0, 1.0);

    // 标题栏命中（非关闭按钮）
    let header_center_x = layout.header.x + 50.0;
    let header_center_y = layout.header.y + 15.0;
    assert!(layout.hit_test_header_drag(header_center_x, header_center_y));

    // 关闭按钮命中
    let close_center_x = layout.close_button.x + layout.close_button.width / 2.0;
    let close_center_y = layout.close_button.y + layout.close_button.height / 2.0;
    assert_eq!(
        layout.hit_test_button(close_center_x, close_center_y),
        Some(OcrEditorButton::CloseHeader)
    );

    // 正文区域命中
    let body_center_x = layout.body.x + 20.0;
    let body_center_y = layout.body.y + 20.0;
    assert!(layout.hit_test_body(body_center_x, body_center_y));
}

#[test]
fn test_ocr_editor_renders_visible_pixels() {
    let editor = OcrTextEditorState::new("可见的识别文字".to_string());
    let mut pixmap = Pixmap::new(800, 600).expect("测试位图应可分配");
    pixmap.fill(Color::TRANSPARENT);
    editor.render(&mut pixmap.as_mut(), 1.0);
    assert!(pixmap
        .data()
        .iter()
        .skip(3)
        .step_by(4)
        .any(|alpha| *alpha > 0));
}
