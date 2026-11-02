//! 主工具栏与拖动手柄绘制。

use super::super::icons::{make_paint, render_item_icon, IconRenderSpec};
use super::super::items::{tool_descriptor, Tool, ToolbarAction, ToolbarItem};
use super::super::metrics::{divider_width, drag_handle_width, has_separator_before, padding};
use super::super::Toolbar;
use super::widgets::{fill_button_highlight, render_active_bottom_bar, render_divider};
use crate::geometry::Rect;
use tiny_skia::{Color, PathBuilder, PixmapMut, Transform};

/// 绘制主栏右端的拖动手柄：前置分隔线 + 2x3 抓握圆点
pub(super) fn render_drag_handle(pixmap: &mut PixmapMut, main_rect: &Rect, s: f32) {
    let left = main_rect.x + main_rect.width - padding(s) - drag_handle_width(s);
    render_divider(pixmap, left + 4.0 * s, main_rect.y + padding(s), s);

    let center_x = left + 12.0 * s;
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
    let s = toolbar.ui_scale;
    let padding = padding(s);
    let items =
        super::super::interaction::main_item_rects(&toolbar.main_rect, toolbar.main_items, s);

    for (idx, (item, btn_rect)) in items.into_iter().enumerate() {
        if has_separator_before(toolbar.main_items, idx) {
            // 按钮矩形已经让过分隔线占位，绘制坐标由矩形反推，与命中区域保持严格一致。
            render_divider(
                pixmap,
                btn_rect.x - divider_width(s) + 3.0 * s,
                toolbar.main_rect.y + padding,
                s,
            );
        }

        let is_geometric_slot = item
            .as_tool()
            .is_some_and(|tool| tool_descriptor(tool).geometric_slot);
        let is_active = if is_geometric_slot {
            matches!(toolbar.active_tool, Some(Tool::Rect | Tool::Ellipse))
        } else {
            item.as_tool()
                .is_some_and(|tool| toolbar.active_tool == Some(tool))
        };
        let is_hovered = toolbar.hovered_item == Some(item);

        if is_active {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(0, 0, 0, 28), s);
            render_active_bottom_bar(pixmap, &btn_rect, s);
        } else if is_hovered {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(0, 0, 0, 16), s);
            if item == ToolbarItem::Action(ToolbarAction::Save) {
                render_active_bottom_bar(pixmap, &btn_rect, s);
            }
        }

        let display_item = toolbar.displayed_item(item);

        render_item_icon(
            pixmap,
            IconRenderSpec {
                item: display_item,
                rect: &btn_rect,
                scale: s,
                active_color: toolbar.sub_options.color,
            },
        );
    }
}
