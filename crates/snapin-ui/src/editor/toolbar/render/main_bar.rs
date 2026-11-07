//! 主工具栏与拖动手柄绘制。

use super::super::icons::{make_paint, render_item_icon, IconRenderSpec};
use super::super::items::{tool_descriptor, Tool, ToolbarAction, ToolbarItem};
use super::super::metrics::{divider_width, drag_handle_width, has_separator_before, padding};
use super::super::Toolbar;
use super::widgets::{fill_button_highlight, render_active_bottom_bar, render_divider};
use crate::geometry::Rect;
use tiny_skia::{Color, PathBuilder, PixmapMut, Transform};

/// 绘制主栏右端的拖动手柄：2x3 抓握圆点
pub(super) fn render_drag_handle(pixmap: &mut PixmapMut, main_rect: &Rect, s: f32) {
    let handle_w = drag_handle_width(s);
    let left = main_rect.x + main_rect.width - padding(s) - handle_w;
    let center_x = left + handle_w / 2.0;
    let center_y = main_rect.y + main_rect.height / 2.0;
    let mut pb = PathBuilder::new();
    for col in [-1.0_f32, 1.0] {
        for row in [-1.0_f32, 0.0, 1.0] {
            pb.push_circle(center_x + col * 2.5 * s, center_y + row * 5.0 * s, 1.4 * s);
        }
    }
    if let Some(path) = pb.finish() {
        let paint = make_paint(Color::from_rgba8(0, 0, 0, 110));
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 绘制主工具栏的各个按钮项
pub(super) fn render_main_buttons(toolbar: &Toolbar, pixmap: &mut PixmapMut) {
    let scale = toolbar.ui_scale;
    let items =
        super::super::interaction::main_item_rects(&toolbar.main_rect, toolbar.main_items, scale);

    for (index, (item, rect)) in items.into_iter().enumerate() {
        render_item_separator(pixmap, toolbar, index, &rect);
        render_item_state(pixmap, toolbar, item, &rect);
        render_item_icon(
            pixmap,
            IconRenderSpec {
                item: toolbar.displayed_item(item),
                rect: &rect,
                scale,
                active_color: toolbar.sub_options.color,
            },
        );
    }
}

/// 绘制按钮前的分隔线。
fn render_item_separator(pixmap: &mut PixmapMut, toolbar: &Toolbar, index: usize, rect: &Rect) {
    let scale = toolbar.ui_scale;
    if has_separator_before(toolbar.main_items, index) {
        // 按钮矩形已经让过分隔线占位，绘制坐标由矩形反推，与命中区域保持严格一致。
        render_divider(
            pixmap,
            rect.x - divider_width(scale) + 3.0 * scale,
            rect.y + rect.height / 2.0,
            scale,
        );
    }
}

/// 绘制按钮的激活、悬停背景与底部指示条。
fn render_item_state(pixmap: &mut PixmapMut, toolbar: &Toolbar, item: ToolbarItem, rect: &Rect) {
    let scale = toolbar.ui_scale;
    if item_is_active(toolbar, item) {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(0, 0, 0, 28), scale);
        render_active_bottom_bar(pixmap, rect, scale);
    } else if toolbar.hovered_item == Some(item) {
        fill_button_highlight(pixmap, rect, Color::from_rgba8(0, 0, 0, 16), scale);
        if item == ToolbarItem::Action(ToolbarAction::Save) {
            render_active_bottom_bar(pixmap, rect, scale);
        }
    }
}

/// 判断工具按钮是否处于激活状态；矩形与椭圆共享几何槽位。
fn item_is_active(toolbar: &Toolbar, item: ToolbarItem) -> bool {
    let is_geometric_slot = item
        .as_tool()
        .is_some_and(|tool| tool_descriptor(tool).geometric_slot);
    if is_geometric_slot {
        matches!(toolbar.active_tool, Some(Tool::Rect | Tool::Ellipse))
    } else {
        item.as_tool()
            .is_some_and(|tool| toolbar.active_tool == Some(tool))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::interaction;
    use super::*;
    use tiny_skia::Pixmap;

    /// 主栏分隔线必须围绕按钮垂直中心绘制，不能继续使用顶部内边距作为纵坐标。
    #[test]
    fn test_main_toolbar_divider_is_vertically_centered() {
        let toolbar = Toolbar {
            main_rect: Rect {
                x: 10.0,
                y: 20.0,
                width: 900.0,
                height: 44.0,
            },
            ..Toolbar::default()
        };
        let items = interaction::main_item_rects(&toolbar.main_rect, toolbar.main_items, 1.0);
        let (_, (_, button_rect)) = items
            .iter()
            .enumerate()
            .find(|(index, _)| has_separator_before(toolbar.main_items, *index))
            .expect("主栏必须存在分隔线");
        let divider_x = (button_rect.x - divider_width(1.0) + 3.0).round() as u32;
        let center_y = (button_rect.y + button_rect.height / 2.0).round() as u32;
        let old_top_y = (toolbar.main_rect.y + padding(1.0)).round() as u32;

        let mut pixmap = Pixmap::new(1000, 100).expect("测试位图应可分配");
        pixmap.fill(Color::TRANSPARENT);
        render_main_buttons(&toolbar, &mut pixmap.as_mut());

        assert!(
            pixmap
                .pixel(divider_x, center_y)
                .expect("中心像素应存在")
                .alpha()
                > 0,
            "主栏分隔线必须在按钮垂直中心可见"
        );
        assert_eq!(
            pixmap
                .pixel(divider_x, old_top_y)
                .expect("旧坐标像素应存在")
                .alpha(),
            0,
            "主栏分隔线不得回到顶部内边距位置"
        );
    }
}
