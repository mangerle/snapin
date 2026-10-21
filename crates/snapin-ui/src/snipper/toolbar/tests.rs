//! 截图工具栏布局计算与交互行为单元测试。

use super::*;
use crate::snipper::selection::Rect;
use crate::snipper::toolbar::items::ToolbarItem;

#[test]
fn test_toolbar_layout_adaptation() {
    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 500.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);
    assert!(tb.main_rect.x > 0.0);
    assert!(tb.main_rect.y >= selection.y + selection.height);

    let bottom_selection = Rect {
        x: 100.0,
        y: 900.0,
        width: 500.0,
        height: 160.0,
    };
    tb.update_layout(&bottom_selection, 1920.0, 1080.0);
    assert!(tb.main_rect.y + tb.main_rect.height <= 1080.0);
}

#[test]
fn test_toolbar_hit_test() {
    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 200.0,
        y: 200.0,
        width: 600.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);

    assert_eq!(tb.hit_test_main(0.0, 0.0), None);

    let first_btn_x = tb.main_rect.x + 10.0;
    let first_btn_y = tb.main_rect.y + 10.0;
    let clicked = tb.handle_click(first_btn_x, first_btn_y);
    assert_eq!(clicked, Some(ToolbarItem::ToolRect));
    assert_eq!(tb.active_tool, Some(ToolbarItem::ToolRect));
    assert!(tb.sub_rect.is_some());
}

#[test]
fn test_toolbar_shape_button_toggles_without_cycling() {
    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 500.0,
        height: 300.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);

    let btn_x = tb.main_rect.x + 10.0;
    let btn_y = tb.main_rect.y + 10.0;

    // 第一次点击主形状按钮：进入矩形（默认形状），并展开二级属性栏
    assert_eq!(tb.handle_click(btn_x, btn_y), Some(ToolbarItem::ToolRect));
    assert_eq!(tb.active_tool, Some(ToolbarItem::ToolRect));
    assert!(tb.sub_rect.is_some(), "形状工具激活后应展开二级属性栏");

    // 再次点击主按钮：取消选择，绝不能切换到椭圆（旧实现的轮转语义已废弃）
    assert_eq!(tb.handle_click(btn_x, btn_y), None);
    assert_eq!(tb.active_tool, None);

    // 第三次点击：重新进入矩形
    assert_eq!(tb.handle_click(btn_x, btn_y), Some(ToolbarItem::ToolRect));

    // 椭圆只能从二级属性栏显式点选：矩形按钮右侧的椭圆按钮
    let sub = tb.sub_rect.expect("二级属性栏应可见");
    let scale = tb.ui_scale;
    let ellipse_x = sub.x + 42.0 * scale;
    let ellipse_y = sub.y + sub.height / 2.0;
    tb.handle_click(ellipse_x, ellipse_y);
    assert_eq!(tb.active_tool, Some(ToolbarItem::ToolEllipse));

    // 椭圆形态下再点主按钮：同样只做关闭
    assert_eq!(tb.handle_click(btn_x, btn_y), None);
    assert_eq!(tb.active_tool, None);
}
