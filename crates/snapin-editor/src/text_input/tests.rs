//! 文本标注会话单元测试。

use super::*;
use tiny_skia::Pixmap;
use winit::keyboard::{Key, NamedKey};

#[test]
fn test_text_input_session_lifecycle() {
    let mut session = TextInputSession::new((50.0, 50.0), (255, 0, 0, 255), 18.0);
    assert_eq!(session.text, "");
    assert_eq!(session.cursor, 0);

    session.push_char('H');
    session.push_str("ello 世界");
    assert_eq!(session.text, "Hello 世界");
    assert_eq!(session.cursor, "Hello 世界".len());

    session.pop_char();
    assert_eq!(session.text, "Hello 世");
    assert_eq!(session.cursor, "Hello 世".len());

    // 验证 IME 预编辑与提交
    session.set_preedit("jie");
    assert_eq!(session.preedit, "jie");
    session.push_commit("界！");
    assert_eq!(session.text, "Hello 世界！");
    assert_eq!(session.preedit, "");

    session.adjust_font_size(4.0);
    assert_eq!(session.font_size, 22.0);

    let action = session.commit();
    assert!(action.is_some());
}

#[test]
fn test_cursor_navigation_and_insert_at_middle() {
    let mut session = TextInputSession::new((0.0, 0.0), (0, 0, 0, 255), 16.0);
    session.insert_str("AC");
    assert_eq!(session.cursor, 2);

    // 左移一位，插入 B
    session.move_cursor_left(false);
    assert_eq!(session.cursor, 1);
    session.insert_char('B');
    assert_eq!(session.text, "ABC");
    assert_eq!(session.cursor, 2);

    // Home 到开头，插入 1
    session.move_cursor_home(false);
    assert_eq!(session.cursor, 0);
    session.insert_str("0");
    assert_eq!(session.text, "0ABC");

    // End 到末尾，Delete 和 Backspace 验证
    session.move_cursor_end(false);
    assert_eq!(session.cursor, 4);
    session.delete_backward();
    assert_eq!(session.text, "0AB");

    session.move_cursor_home(false);
    session.delete_forward();
    assert_eq!(session.text, "AB");
}

#[test]
fn test_selection_and_replace() {
    let mut session = TextInputSession::new((0.0, 0.0), (0, 0, 0, 255), 16.0);
    session.insert_str("Hello World");

    // 全选
    session.select_all();
    assert_eq!(session.selection_range(), Some((0, 11)));
    assert_eq!(session.selected_text(), Some("Hello World"));

    // 选区输入直接替换
    session.insert_str("Hi");
    assert_eq!(session.text, "Hi");
    assert_eq!(session.selection_range(), None);
    assert_eq!(session.cursor, 2);

    // 方向键选择：左移两位并选中
    session.move_cursor_left(true);
    assert_eq!(session.selected_text(), Some("i"));
    session.delete_selection();
    assert_eq!(session.text, "H");
}

#[test]
fn test_key_handling() {
    let mut session = TextInputSession::new((0.0, 0.0), (255, 255, 255, 255), 16.0);
    let normal = KeyModifiers::default();
    let shift = KeyModifiers::new(false, true, false);
    let ctrl = KeyModifiers::new(true, false, false);

    assert_eq!(
        handle_session_key(&mut session, &Key::Named(NamedKey::Escape), normal),
        TextSessionAction::Cancel
    );
    assert_eq!(
        handle_session_key(&mut session, &Key::Named(NamedKey::Enter), shift),
        TextSessionAction::Handled
    );
    assert_eq!(session.text, "\n");
    assert_eq!(
        handle_session_key(&mut session, &Key::Named(NamedKey::Enter), normal),
        TextSessionAction::Commit
    );

    // 字符输入
    assert_eq!(
        handle_session_key(&mut session, &Key::Character("a".into()), normal),
        TextSessionAction::Handled
    );
    assert_eq!(session.text, "\na");

    // Ctrl+A, Ctrl+C, Ctrl+V, Ctrl+X
    assert_eq!(
        handle_session_key(&mut session, &Key::Character("a".into()), ctrl),
        TextSessionAction::Handled
    );
    assert!(session.selection_range().is_some());

    match handle_session_key(&mut session, &Key::Character("c".into()), ctrl) {
        TextSessionAction::Copy(text) => assert_eq!(text, "\na"),
        other => panic!("预期复制动作，实际为 {other:?}"),
    }

    assert_eq!(
        handle_session_key(&mut session, &Key::Character("v".into()), ctrl),
        TextSessionAction::PasteRequest
    );

    match handle_session_key(&mut session, &Key::Character("x".into()), ctrl) {
        TextSessionAction::Cut(text) => {
            assert_eq!(text, "\na");
            assert_eq!(session.text, "");
        }
        other => panic!("预期剪切动作，实际为 {other:?}"),
    }
}

#[test]
fn test_hit_test_and_contains_point() {
    let mut session = TextInputSession::new((10.0, 10.0), (0, 0, 0, 255), 20.0);
    session.insert_str("First\nSecond");

    assert!(session.contains_screen_point(15.0, 15.0, 1.0));
    assert!(!session.contains_screen_point(500.0, 500.0, 1.0));

    // 点击测试
    session.set_cursor_from_screen_point(10.0, 10.0, false, 1.0);
    assert_eq!(session.cursor, 0);

    // 点击末尾
    session.set_cursor_from_screen_point(300.0, 50.0, false, 1.0);
    assert_eq!(session.cursor, session.text.len());
}

#[test]
fn test_render_pixels() {
    let mut pixmap = Pixmap::new(200, 100).unwrap();
    pixmap.fill(tiny_skia::Color::BLACK);

    let mut session = TextInputSession::new((10.0, 20.0), (255, 255, 255, 255), 20.0);
    session.insert_str("测试文本");
    session.render(&mut pixmap.as_mut());

    let has_content = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
    assert!(has_content);
}
