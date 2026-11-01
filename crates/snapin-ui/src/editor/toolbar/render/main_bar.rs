//! 主工具栏与拖动手柄绘制。

use super::super::icons::{make_paint, render_item_icon};
use super::super::items::ToolbarItem;
use super::super::metrics::{
    button_size, divider_width, drag_handle_width, has_separator_before, padding, spacing,
};
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
    let btn_size = button_size(s);
    let padding = padding(s);
    let spacing = spacing(s);
    let mut cur_x = toolbar.main_rect.x + padding;

    for (idx, &item) in toolbar.main_items.iter().enumerate() {
        if has_separator_before(toolbar.main_items, idx) {
            render_divider(pixmap, cur_x + 3.0 * s, toolbar.main_rect.y + padding, s);
            cur_x += divider_width(s);
        }

        let btn_rect = Rect {
            x: cur_x,
            y: toolbar.main_rect.y + padding,
            width: btn_size,
            height: btn_size,
        };

        let is_geometric_slot = item == ToolbarItem::ToolRect;
        let is_active = if is_geometric_slot {
            toolbar.active_tool == Some(ToolbarItem::ToolRect)
                || toolbar.active_tool == Some(ToolbarItem::ToolEllipse)
        } else {
            toolbar.active_tool == Some(item)
        };
        let is_hovered = toolbar.hovered_item == Some(item);

        if is_active {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(0, 0, 0, 28), s);
            render_active_bottom_bar(pixmap, &btn_rect, s);
        } else if is_hovered {
            fill_button_highlight(pixmap, &btn_rect, Color::from_rgba8(0, 0, 0, 16), s);
            if item == ToolbarItem::ActionSave {
                render_active_bottom_bar(pixmap, &btn_rect, s);
            }
        }

        let display_item =
            if is_geometric_slot && toolbar.active_tool == Some(ToolbarItem::ToolEllipse) {
                ToolbarItem::ToolEllipse
            } else {
                item
            };

        render_item_icon(
            pixmap,
            display_item,
            &btn_rect,
            s,
            toolbar.sub_options.color,
        );
        cur_x += btn_size + spacing;
    }
}
