//! 右键上下文菜单与文字渲染单元测试。

use super::*;

#[test]
fn test_context_menu_open_and_close() {
    let mut menu = ContextMenuState::default();
    let items = ContextMenuState::build_main_items(true, false);
    assert!(!menu.is_open);
    menu.open_at(100.0, 150.0, 1000.0, 1000.0, &items);
    assert!(menu.is_open);
    assert_eq!(menu.x, 100.0);
    assert_eq!(menu.y, 150.0);
    menu.close();
    assert!(!menu.is_open);

    // 边界避让测试：在底部 y=950 处点击，由于超出 1000，自动向上翻折
    menu.open_at(100.0, 950.0, 1000.0, 1000.0, &items);
    assert!(menu.y < 950.0);
}

#[test]
fn test_context_menu_items_structure() {
    let items = ContextMenuState::build_main_items(true, false);
    assert!(items.len() >= 10);
    let sub_scale = ContextMenuState::build_sub_items(SubMenuType::Scale, true);
    assert_eq!(sub_scale.len(), 6);
    let sub_img = ContextMenuState::build_sub_items(SubMenuType::ImageProcessing, true);
    assert_eq!(sub_img.len(), 4);
}

#[test]
fn test_context_menu_click_action() {
    let mut menu = ContextMenuState::default();
    let items = ContextMenuState::build_main_items(false, true);
    menu.open_at(50.0, 50.0, 1000.0, 1000.0, &items);

    // 点击第一项 "复制图像" (y 在 55.0 ~ 83.0 之间)
    let clicked = menu.handle_click(60.0, 65.0, &items, true);
    assert_eq!(clicked, Some(MenuItemId::CopyImage));
    assert!(!menu.is_open);
}

#[test]
fn test_text_renderer_draws_pixels() {
    let mut tr = render::TextRenderer::default();
    let mut pixmap = tiny_skia::Pixmap::new(200, 100).unwrap();
    pixmap.fill(tiny_skia::Color::from_rgba8(26, 28, 40, 255));
    let before = pixmap.data().to_vec();
    tr.draw_text(
        &mut pixmap.as_mut(),
        "复制图像",
        10.0,
        10.0,
        14.0,
        tiny_skia::Color::WHITE,
    );
    let after = pixmap.data();
    let changed = before.iter().zip(after.iter()).any(|(a, b)| a != b);
    assert!(changed, "draw_text 应该绘制出了像素！");
}

/// 回归：菜单所需窗口尺寸必须是绝对占位，不得叠加光标坐标。
///
/// 旧实现返回 `(光标x + 菜单宽, 光标y + 菜单高)`，只要在贴图右下角右键就会把窗口无谓撑大；
/// 撑出的空区域既增加重绘与合成开销，也会拦截本应落到桌面的鼠标事件
/// （透明像素同样参与 Wayland 表面输入命中）。
#[test]
fn test_required_menu_extent_is_cursor_independent() {
    let menu = ContextMenuState::default();
    let items = ContextMenuState::build_main_items(true, true);
    let (extent_w, extent_h) = menu.required_menu_extent(&items);

    let scale = menu.ui_scale;
    let expected_h =
        ContextMenuState::items_total_height(&items, menu.item_height, scale) + 20.0 * scale;
    assert!(
        (extent_h - expected_h).abs() < 0.01,
        "高度应只等于菜单自身占位，实际 {extent_h}，期望 {expected_h}"
    );
    assert!(
        extent_w >= (170.0 + 160.0) * scale,
        "宽度需容纳主菜单与二级子菜单，实际 {extent_w}"
    );
    // 尺寸与光标无关：签名已不含坐标参数，任何位置右键都得到同一占位，不会因右下角点击而膨胀
}
