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

/// 线型下拉：点击按钮展开菜单，点击"虚线"行后写回样式并收起菜单
#[test]
fn test_line_style_dropdown_selects_style() {
    use snapin_core::annotation::LineStyle;

    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 600.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);

    // 激活矩形工具以展开二级栏
    let btn_x = tb.main_rect.x + 10.0;
    let btn_y = tb.main_rect.y + 10.0;
    tb.handle_click(btn_x, btn_y);

    let layout = tb.sub_layout().expect("矩形工具应展开二级栏");
    let line_btn = layout
        .rect_of(SubControl::LineStyle)
        .expect("二级栏应包含线型控件");
    tb.handle_click(
        line_btn.x + line_btn.width / 2.0,
        line_btn.y + line_btn.height / 2.0,
    );
    assert!(tb.open_dropdown.is_some(), "点击线型按钮应展开下拉菜单");

    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("应生成线型下拉菜单");
    assert_eq!(dropdown.rows.len(), LineStyle::ALL.len());

    // 第二行为虚线
    let row = dropdown.rows[1];
    tb.handle_click(row.x + 4.0, row.y + row.height / 2.0);
    assert_eq!(tb.sub_options.line_style, LineStyle::Dashed);
    assert!(tb.open_dropdown.is_none(), "选中后菜单应收起");

    // 再次打开并选择点线
    let layout = tb.sub_layout().expect("二级栏仍在");
    let line_btn = layout.rect_of(SubControl::LineStyle).expect("线型控件仍在");
    tb.handle_click(
        line_btn.x + line_btn.width / 2.0,
        line_btn.y + line_btn.height / 2.0,
    );
    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("应再次生成下拉菜单");
    let row = dropdown.rows[2];
    tb.handle_click(row.x + 4.0, row.y + row.height / 2.0);
    assert_eq!(tb.sub_options.line_style, LineStyle::Dotted);
}

/// 填充开关与序号底板样式：点击即切换，且与工具类型正确关联
#[test]
fn test_fill_toggle_and_badge_style() {
    use snapin_core::annotation::BadgeStyle;

    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 600.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);

    let btn_x = tb.main_rect.x + 10.0;
    let btn_y = tb.main_rect.y + 10.0;
    tb.handle_click(btn_x, btn_y);

    let fill = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::FillToggle))
        .expect("矩形工具应提供填充开关");
    assert!(!tb.sub_options.filled);
    tb.handle_click(fill.x + fill.width / 2.0, fill.y + fill.height / 2.0);
    assert!(tb.sub_options.filled, "首次点击应切换为填充");
    tb.handle_click(fill.x + fill.width / 2.0, fill.y + fill.height / 2.0);
    assert!(!tb.sub_options.filled, "再次点击应恢复仅描边");

    // 切换到序号工具：应出现底板样式控件，且不再有填充/圆角
    let badge_index = tb
        .main_items
        .iter()
        .position(|item| *item == ToolbarItem::ToolBadge)
        .expect("主栏应包含序号工具");
    let badge_x = tb.main_rect.x
        + super::metrics::padding(tb.ui_scale)
        + badge_index as f32
            * (super::metrics::button_size(tb.ui_scale) + super::metrics::spacing(tb.ui_scale));
    tb.handle_click(
        badge_x + 2.0,
        tb.main_rect.y + super::metrics::padding(tb.ui_scale) + 2.0,
    );
    assert_eq!(tb.active_tool, Some(ToolbarItem::ToolBadge));

    let layout = tb.sub_layout().expect("序号工具应展开二级栏");
    assert!(layout.rect_of(SubControl::FillToggle).is_none());
    let style_btn = layout
        .rect_of(SubControl::BadgeStyle)
        .expect("序号工具应提供底板样式控件");
    tb.handle_click(
        style_btn.x + style_btn.width / 2.0,
        style_btn.y + style_btn.height / 2.0,
    );
    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("应生成底板样式下拉菜单");
    assert_eq!(dropdown.rows.len(), 2);
    let row = dropdown.rows[1];
    tb.handle_click(row.x + 4.0, row.y + row.height / 2.0);
    assert_eq!(tb.sub_options.badge_style, BadgeStyle::Outline);
}

/// 下拉菜单展开时点击主栏其他工具，应先收起菜单再切换工具
#[test]
fn test_dropdown_closes_when_clicking_other_tool() {
    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 600.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);

    let btn_x = tb.main_rect.x + 10.0;
    let btn_y = tb.main_rect.y + 10.0;
    tb.handle_click(btn_x, btn_y);

    let line_btn = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::LineStyle))
        .expect("矩形工具应提供线型控件");
    tb.handle_click(line_btn.x + 2.0, line_btn.y + 2.0);
    assert!(tb.open_dropdown.is_some());

    // 点击画笔工具
    let pen_index = tb
        .main_items
        .iter()
        .position(|item| *item == ToolbarItem::ToolPen)
        .expect("主栏应包含画笔");
    let pen_x = tb.main_rect.x
        + super::metrics::padding(tb.ui_scale)
        + pen_index as f32
            * (super::metrics::button_size(tb.ui_scale) + super::metrics::spacing(tb.ui_scale));
    tb.handle_click(
        pen_x + 2.0,
        tb.main_rect.y + super::metrics::padding(tb.ui_scale) + 2.0,
    );

    assert!(tb.open_dropdown.is_none(), "切换工具后下拉菜单应收起");
    assert_eq!(tb.active_tool, Some(ToolbarItem::ToolPen));
}

/// 箭头样式下拉：实线/虚线/锥形三选一，选中后写回并收起菜单
#[test]
fn test_arrow_style_dropdown_selects_style() {
    use snapin_core::annotation::ArrowStyle;

    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 600.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);

    // 激活箭头工具
    let arrow_index = tb
        .main_items
        .iter()
        .position(|item| *item == ToolbarItem::ToolArrow)
        .expect("主栏应包含箭头");
    let arrow_x = tb.main_rect.x
        + super::metrics::padding(tb.ui_scale)
        + arrow_index as f32
            * (super::metrics::button_size(tb.ui_scale) + super::metrics::spacing(tb.ui_scale));
    tb.handle_click(
        arrow_x + 2.0,
        tb.main_rect.y + super::metrics::padding(tb.ui_scale) + 2.0,
    );
    assert_eq!(tb.active_tool, Some(ToolbarItem::ToolArrow));
    assert_eq!(
        tb.sub_options.arrow_style,
        ArrowStyle::Tapered,
        "箭头默认应为锥形"
    );

    let style_btn = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::ArrowStyle))
        .expect("箭头工具应提供样式控件");
    tb.handle_click(
        style_btn.x + style_btn.width / 2.0,
        style_btn.y + style_btn.height / 2.0,
    );
    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("应生成箭头样式下拉菜单");
    assert_eq!(dropdown.rows.len(), ArrowStyle::ALL.len());

    // 第一行为实线箭头
    let row = dropdown.rows[0];
    tb.handle_click(row.x + 4.0, row.y + row.height / 2.0);
    assert_eq!(tb.sub_options.arrow_style, ArrowStyle::Solid);
    assert!(tb.open_dropdown.is_none());
}

/// 圆角控件可点击：展开预设档位并写回半径
#[test]
fn test_corner_radius_dropdown_selects_preset() {
    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 600.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);

    let btn_x = tb.main_rect.x + 10.0;
    let btn_y = tb.main_rect.y + 10.0;
    tb.handle_click(btn_x, btn_y);

    let corner = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::CornerRadius))
        .expect("矩形工具应提供圆角控件");
    tb.handle_click(
        corner.x + corner.width / 2.0,
        corner.y + corner.height / 2.0,
    );
    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("点击圆角控件应展开预设下拉");
    assert_eq!(dropdown.rows.len(), CORNER_RADIUS_PRESETS.len());

    // 第二档预设（6px）
    let row = dropdown.rows[1];
    tb.handle_click(row.x + 4.0, row.y + row.height / 2.0);
    assert_eq!(tb.sub_options.corner_radius, CORNER_RADIUS_PRESETS[1]);
    assert!(tb.open_dropdown.is_none(), "选中后菜单应收起");
}
