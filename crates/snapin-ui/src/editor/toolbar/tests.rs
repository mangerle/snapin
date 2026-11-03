//! 截图工具栏布局计算与交互行为单元测试。

use super::*;
use crate::editor::toolbar::items::preset_colors;
use crate::editor::toolbar::items::{Tool, ToolbarItem};
use crate::editor::toolbar::layout::SubControl;
use crate::geometry::Rect;

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
    assert_eq!(clicked, None, "工具点击只改变激活态，不产生一次性动作");
    assert_eq!(tb.active_tool, Some(Tool::Rect));
    assert!(tb.sub_rect.is_some());
}

/// 文字工具二级栏必须提供字号下拉，并可写回预设字号。
#[test]
fn test_text_font_size_dropdown_sets_font_size() {
    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 500.0,
        height: 300.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);
    let text_rect = interaction::main_item_rects(&tb.main_rect, tb.main_items, tb.ui_scale)
        .into_iter()
        .find(|(item, _)| *item == ToolbarItem::Tool(Tool::Text))
        .map(|(_, rect)| rect)
        .expect("文字按钮应存在");
    tb.handle_click(text_rect.x + 5.0, text_rect.y + 5.0);

    let layout = tb.sub_layout().expect("文字工具必须展开二级栏");
    let font_rect = layout
        .rect_of(SubControl::FontSize)
        .expect("文字工具必须有字号控件");
    tb.handle_click(font_rect.x + 5.0, font_rect.y + 5.0);
    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.as_ref())
        .expect("字号下拉应展开");
    let row = dropdown.rows.get(4).expect("字号下拉应有 32px 预设");
    tb.handle_click(row.x + 4.0, row.y + 4.0);
    assert_eq!(tb.sub_options.font_size, 32.0);
}

/// 马赛克二级栏的智能打码按钮返回一次性动作，不切换工具状态。
#[test]
fn test_mosaic_smart_redact_sub_control_returns_action() {
    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 500.0,
        height: 300.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);
    let mosaic_rect = interaction::main_item_rects(&tb.main_rect, tb.main_items, tb.ui_scale)
        .into_iter()
        .find(|(item, _)| *item == ToolbarItem::Tool(Tool::Mosaic))
        .map(|(_, rect)| rect)
        .expect("马赛克按钮应存在");
    tb.handle_click(mosaic_rect.x + 5.0, mosaic_rect.y + 5.0);
    let layout = tb.sub_layout().expect("马赛克工具必须展开二级栏");
    let mode_rect = layout
        .rect_of(SubControl::MosaicMode)
        .expect("必须提供遮挡模式入口");
    tb.handle_click(
        mode_rect.x + mode_rect.width / 2.0,
        mode_rect.y + mode_rect.height / 2.0,
    );
    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("遮挡模式必须提供下拉菜单");
    assert_eq!(
        tb.handle_click(dropdown.rows[2].x + 5.0, dropdown.rows[2].y + 5.0),
        Some(ToolbarAction::SmartRedact)
    );
    assert_eq!(tb.active_tool, Some(Tool::Mosaic));
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
    assert_eq!(
        tb.handle_click(btn_x, btn_y),
        None,
        "工具点击只改变激活态，不产生一次性动作"
    );
    assert_eq!(tb.active_tool, Some(Tool::Rect));
    assert!(tb.sub_rect.is_some(), "形状工具激活后应展开二级属性栏");

    // 再次点击主按钮：取消选择，绝不能切换到椭圆（旧实现的轮转语义已废弃）
    assert_eq!(tb.handle_click(btn_x, btn_y), None);
    assert_eq!(tb.active_tool, None);

    // 第三次点击：重新进入矩形
    assert_eq!(
        tb.handle_click(btn_x, btn_y),
        None,
        "工具点击只改变激活态，不产生一次性动作"
    );

    // 椭圆只能从二级属性栏显式点选：矩形按钮右侧的椭圆按钮
    let ellipse = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::ShapeEllipse))
        .expect("形状二级栏应包含椭圆按钮");
    tb.handle_click(
        ellipse.x + ellipse.width / 2.0,
        ellipse.y + ellipse.height / 2.0,
    );
    assert_eq!(tb.active_tool, Some(Tool::Ellipse));

    // 椭圆形态下再点主按钮：同样只做关闭
    assert_eq!(tb.handle_click(btn_x, btn_y), None);
    assert_eq!(tb.active_tool, None);
}

/// 序号形态和数字形态切换都不得修改颜色；默认颜色应保持红色选中。
#[test]
fn test_badge_dropdowns_preserve_default_red_color() {
    use snapin_annotation::{BadgeSequence, BadgeStyle};

    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 500.0,
        height: 300.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);
    assert_eq!(
        tb.sub_options.color,
        preset_colors()[0],
        "默认颜色必须是红色"
    );

    let badge_rect = interaction::main_item_rects(&tb.main_rect, tb.main_items, tb.ui_scale)
        .into_iter()
        .find(|(item, _)| *item == ToolbarItem::Tool(Tool::Badge))
        .map(|(_, rect)| rect)
        .expect("主栏应包含序号注释");
    tb.handle_click(badge_rect.x + 5.0, badge_rect.y + 5.0);

    let style = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::BadgeStyle))
        .expect("序号二级栏应包含形态控件");
    tb.handle_click(style.x + 5.0, style.y + 5.0);
    let style_dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("序号形态应展开下拉");
    tb.handle_click(
        style_dropdown.rows[1].x + 4.0,
        style_dropdown.rows[1].y + 5.0,
    );
    assert_eq!(tb.sub_options.badge_style, BadgeStyle::Outline);
    assert_eq!(
        tb.sub_options.color,
        preset_colors()[0],
        "切换形态不得改变颜色"
    );

    let sequence = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::BadgeSequence))
        .expect("序号二级栏应包含数字形态控件");
    tb.handle_click(sequence.x + 5.0, sequence.y + 5.0);
    let sequence_dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("数字形态应展开下拉");
    tb.handle_click(
        sequence_dropdown.rows[1].x + 4.0,
        sequence_dropdown.rows[1].y + 5.0,
    );
    assert_eq!(
        tb.sub_options.badge_sequence,
        BadgeSequence::UppercaseLetter
    );
    assert_eq!(
        tb.sub_options.color,
        preset_colors()[0],
        "切换数字形态不得改变颜色"
    );
}

/// 荧光笔的形状和混合模式都应通过下拉选择，并显示当前选项。
#[test]
fn test_marker_dropdowns_change_shape_and_blend_mode() {
    use snapin_annotation::{MarkerBlendMode, MarkerShape};

    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 500.0,
        height: 300.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);
    let marker_rect = interaction::main_item_rects(&tb.main_rect, tb.main_items, tb.ui_scale)
        .into_iter()
        .find(|(item, _)| *item == ToolbarItem::Tool(Tool::Marker))
        .map(|(_, rect)| rect)
        .expect("主栏应包含荧光笔");
    tb.handle_click(marker_rect.x + 5.0, marker_rect.y + 5.0);

    let shape = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::MarkerShape))
        .expect("荧光笔应提供形状下拉");
    tb.handle_click(shape.x + 5.0, shape.y + 5.0);
    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("形状控件应展开下拉");
    assert_eq!(dropdown.rows.len(), 2);
    tb.handle_click(dropdown.rows[1].x + 4.0, dropdown.rows[1].y + 5.0);
    assert_eq!(tb.sub_options.marker_shape, MarkerShape::Rectangle);

    let blend = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::MarkerBlendMode))
        .expect("荧光笔应提供混合模式下拉");
    tb.handle_click(blend.x + 5.0, blend.y + 5.0);
    let dropdown = tb
        .sub_layout()
        .and_then(|layout| layout.dropdown.clone())
        .expect("混合模式应展开下拉");
    tb.handle_click(dropdown.rows[1].x + 4.0, dropdown.rows[1].y + 5.0);
    assert_eq!(tb.sub_options.marker_blend_mode, MarkerBlendMode::Normal);
}

/// 马赛克强度应支持点击滑块位置直接设置数值。
#[test]
fn test_mosaic_effect_strength_slider_sets_value() {
    let mut tb = Toolbar::default();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 500.0,
        height: 300.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);
    let mosaic_rect = interaction::main_item_rects(&tb.main_rect, tb.main_items, tb.ui_scale)
        .into_iter()
        .find(|(item, _)| *item == ToolbarItem::Tool(Tool::Mosaic))
        .map(|(_, rect)| rect)
        .expect("主栏应包含马赛克");
    tb.handle_click(mosaic_rect.x + 5.0, mosaic_rect.y + 5.0);
    let slider = tb
        .sub_layout()
        .and_then(|layout| layout.rect_of(SubControl::EffectStrength))
        .expect("马赛克应提供效果强度滑块");
    tb.handle_click(
        slider.x + slider.width / 2.0,
        slider.y + slider.height / 2.0,
    );
    assert!((tb.sub_options.effect_strength as i16 - 88).abs() <= 2);
}

/// 线型下拉：点击按钮展开菜单，点击"虚线"行后写回样式并收起菜单
#[test]
fn test_line_style_dropdown_selects_style() {
    use snapin_annotation::LineStyle;

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
    use snapin_annotation::BadgeStyle;

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
        .position(|item| *item == ToolbarItem::Tool(Tool::Badge))
        .expect("主栏应包含序号工具");
    let badge_x = tb.main_rect.x
        + super::metrics::padding(tb.ui_scale)
        + badge_index as f32
            * (super::metrics::button_size(tb.ui_scale) + super::metrics::spacing(tb.ui_scale));
    tb.handle_click(
        badge_x + 2.0,
        tb.main_rect.y + super::metrics::padding(tb.ui_scale) + 2.0,
    );
    assert_eq!(tb.active_tool, Some(Tool::Badge));

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

    // 点击未被下拉层覆盖的序号工具；使用真实按钮热区，避免手工推算漏掉分隔线占位。
    let badge_rect = interaction::main_item_rects(&tb.main_rect, tb.main_items, tb.ui_scale)
        .into_iter()
        .find(|(item, _)| *item == ToolbarItem::Tool(Tool::Badge))
        .map(|(_, rect)| rect)
        .expect("主栏应包含序号工具");
    tb.handle_click(badge_rect.x + 2.0, badge_rect.y + 2.0);

    assert!(tb.open_dropdown.is_none(), "切换工具后下拉菜单应收起");
    assert_eq!(tb.active_tool, Some(Tool::Badge));
}

/// 箭头样式下拉：实线/虚线/锥形三选一，选中后写回并收起菜单
#[test]
fn test_arrow_style_dropdown_selects_style() {
    use snapin_annotation::ArrowStyle;

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
        .position(|item| *item == ToolbarItem::Tool(Tool::Arrow))
        .expect("主栏应包含箭头");
    let arrow_x = tb.main_rect.x
        + super::metrics::padding(tb.ui_scale)
        + arrow_index as f32
            * (super::metrics::button_size(tb.ui_scale) + super::metrics::spacing(tb.ui_scale));
    tb.handle_click(
        arrow_x + 2.0,
        tb.main_rect.y + super::metrics::padding(tb.ui_scale) + 2.0,
    );
    assert_eq!(tb.active_tool, Some(Tool::Arrow));
    assert_eq!(
        tb.sub_options.arrow_style,
        ArrowStyle::Solid,
        "箭头默认应为实线，保证线型设置立即生效"
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

/// 圆角控件点击后保持焦点，光标移出二级栏后滚轮仍连续调整圆角数值。
#[test]
fn test_corner_radius_click_then_wheel_changes_value() {
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
    assert_eq!(tb.focused_sub_control, Some(SubControl::CornerRadius));
    assert!(tb.open_dropdown.is_none(), "圆角控件不再展开下拉菜单");

    // 点击圆角后即使光标移出二级栏，滚轮仍应继续调整圆角，而不是回落为粗细。
    assert!(tb.handle_sub_wheel(0.0, 0.0, 1.0));
    assert_eq!(tb.sub_options.corner_radius, 2.0);
    assert!(tb.handle_sub_wheel(0.0, 0.0, 1.0));
    assert_eq!(tb.sub_options.corner_radius, 4.0);
}

/// 拖动手柄：命中右端手柄才能开始拖动，拖动后位置夹取到屏幕内，
/// 调整选区时保持手动位置，重置后恢复自适应摆放
#[test]
fn test_drag_handle_moves_and_keeps_manual_position() {
    let mut tb = Toolbar {
        draggable: true,
        ..Toolbar::default()
    };
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 800.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);
    let auto_rect = tb.main_rect;

    // 首个按钮不是手柄，不能开始拖动；按钮命中也不受手柄影响
    assert!(!tb.begin_drag(auto_rect.x + 10.0, auto_rect.y + 10.0));
    let handle_x = auto_rect.x + auto_rect.width - 10.0;
    let handle_y = auto_rect.y + auto_rect.height / 2.0;
    assert_eq!(tb.hit_test_main(handle_x, handle_y), None);

    assert!(tb.begin_drag(handle_x, handle_y));
    assert!(tb.is_dragging());
    assert!(tb.drag_to(handle_x - 50.0, handle_y - 300.0));
    assert!((tb.main_rect.x - (auto_rect.x - 50.0)).abs() < 0.01);
    assert!((tb.main_rect.y - (auto_rect.y - 300.0)).abs() < 0.01);

    // 拖出屏幕时夹取在可视范围内
    tb.drag_to(-5000.0, -5000.0);
    assert_eq!((tb.main_rect.x, tb.main_rect.y), (0.0, 0.0));
    assert!(tb.end_drag());
    assert!(!tb.end_drag(), "重复释放不应再被视为拖动");

    // 选区变化时保持手动位置
    tb.update_layout(&selection, 1920.0, 1080.0);
    assert_eq!((tb.main_rect.x, tb.main_rect.y), (0.0, 0.0));

    // 重置后恢复自适应摆放
    tb.reset_manual_position();
    tb.update_layout(&selection, 1920.0, 1080.0);
    assert_eq!(tb.main_rect, auto_rect);
}

/// 未启用拖动的工具栏（贴图）不显示手柄、宽度不变
#[test]
fn test_drag_handle_disabled_by_default() {
    let mut tb = Toolbar::default();
    let width_without = tb.main_width();
    let selection = Rect {
        x: 100.0,
        y: 100.0,
        width: 800.0,
        height: 400.0,
    };
    tb.update_layout(&selection, 1920.0, 1080.0);
    let rect = tb.main_rect;
    assert!(!tb.begin_drag(rect.x + rect.width - 10.0, rect.y + rect.height / 2.0));

    tb.draggable = true;
    assert!(tb.main_width() > width_without);
}
