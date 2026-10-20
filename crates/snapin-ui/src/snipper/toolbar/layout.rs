//! 截图工具栏与二级属性栏自适应布局计算。
//!
//! 根据用户框选的屏幕选区几何位置与当前屏幕边界，自适应将工具栏定位在选区底部或顶部，
//! 并确保在靠近屏幕边缘时自动收缩贴边，防止 UI 超出可视区域。

use crate::snipper::selection::Rect;
use crate::snipper::toolbar::items::ToolbarItem;
use crate::snipper::toolbar::metrics::{button_size, main_toolbar_width, padding};

/// 根据选区与屏幕宽高重新计算主工具栏在屏幕上的自适应摆放位置
pub(crate) fn calculate_main_layout(
    selection: &Rect,
    screen_w: f32,
    screen_h: f32,
    ui_scale: f32,
    total_items: usize,
) -> Rect {
    let s = ui_scale;
    let width = main_toolbar_width(total_items, s);
    let height = padding(s) * 2.0 + button_size(s);

    let mut x = (selection.x + selection.width) - width;
    let mut y = selection.y + selection.height + 8.0 * s;

    if x + width > screen_w - 8.0 * s {
        x = screen_w - width - 8.0 * s;
    }
    if x < 8.0 * s {
        x = 8.0 * s;
    }

    if y + height > screen_h - 8.0 * s {
        let above_y = selection.y - height - 8.0 * s;
        if above_y >= 8.0 * s {
            y = above_y;
        } else {
            y = (selection.y + selection.height - height - 8.0 * s).max(8.0 * s);
        }
    }

    Rect {
        x,
        y,
        width,
        height,
    }
}

/// 根据当前主工具栏位置重新定位二级属性栏
pub(crate) fn calculate_sub_layout(
    main_rect: &Rect,
    active_tool: Option<ToolbarItem>,
    ui_scale: f32,
) -> Option<Rect> {
    if active_tool.is_none() || main_rect.width <= 0.0 {
        return None;
    }

    let s = ui_scale;
    let sub_h = 36.0 * s;
    let is_geo =
        active_tool == Some(ToolbarItem::ToolRect) || active_tool == Some(ToolbarItem::ToolEllipse);
    let sub_w = if is_geo { 400.0 * s } else { 340.0 * s };
    let sub_x = main_rect.x;
    let sub_y = if main_rect.y >= sub_h + 8.0 * s {
        main_rect.y - sub_h - 4.0 * s
    } else {
        main_rect.y + main_rect.height + 4.0 * s
    };

    Some(Rect {
        x: sub_x,
        y: sub_y,
        width: sub_w,
        height: sub_h,
    })
}
