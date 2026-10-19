//! 截图工具栏鼠标交互与事件分发。
//!
//! 负责工具栏各按钮的鼠标悬停碰撞检测、点击工具切换、
//! 二级属性栏颜色与线宽调整，以及滚轮快捷阶梯切换线宽。

use crate::snipper::selection::Rect;
use crate::snipper::toolbar::items::{
    preset_colors, StrokeWidthLevel, SubToolOptions, ToolbarItem,
};

/// 主工具栏按钮命中测试 (计算每个按钮的热区并进行点碰撞检测)
pub(crate) fn hit_test_main_items(
    main_rect: &Rect,
    items: &[ToolbarItem],
    ui_scale: f32,
    px: f32,
    py: f32,
) -> Option<ToolbarItem> {
    if !main_rect.contains(px, py) {
        return None;
    }

    let s = ui_scale;
    let btn_size = 32.0 * s;
    let padding = 6.0 * s;
    let spacing = 4.0 * s;
    let mut cur_x = main_rect.x + padding;

    for (idx, &item) in items.iter().enumerate() {
        if idx == 7 || idx == 11 {
            cur_x += 8.0 * s;
        }

        let btn_rect = Rect {
            x: cur_x,
            y: main_rect.y + padding,
            width: btn_size,
            height: btn_size,
        };

        if btn_rect.contains(px, py) {
            return Some(item);
        }

        cur_x += btn_size + spacing;
    }

    None
}

/// 处理二级属性栏内的几何形状、粗细与颜色点击
pub(crate) fn handle_sub_click(
    sub_rect: Rect,
    active_tool: &mut Option<ToolbarItem>,
    sub_options: &mut SubToolOptions,
    ui_scale: f32,
    px: f32,
    py: f32,
) {
    let s = ui_scale;
    let padding = 8.0 * s;
    let mut cur_x = sub_rect.x + padding;
    let center_y = sub_rect.y + sub_rect.height / 2.0;

    let is_geo = *active_tool == Some(ToolbarItem::ToolRect)
        || *active_tool == Some(ToolbarItem::ToolEllipse);
    if is_geo {
        let rect_btn = Rect {
            x: cur_x - 4.0 * s,
            y: center_y - 12.0 * s,
            width: 24.0 * s,
            height: 24.0 * s,
        };
        if rect_btn.contains(px, py) {
            *active_tool = Some(ToolbarItem::ToolRect);
            return;
        }
        cur_x += 26.0 * s;

        let ellipse_btn = Rect {
            x: cur_x - 4.0 * s,
            y: center_y - 12.0 * s,
            width: 24.0 * s,
            height: 24.0 * s,
        };
        if ellipse_btn.contains(px, py) {
            *active_tool = Some(ToolbarItem::ToolEllipse);
            return;
        }
        cur_x += 26.0 * s + 16.0 * s;
    }

    for level in [
        StrokeWidthLevel::Thin,
        StrokeWidthLevel::Medium,
        StrokeWidthLevel::Thick,
    ] {
        let hit_box = Rect {
            x: cur_x - 6.0 * s,
            y: center_y - 12.0 * s,
            width: 24.0 * s,
            height: 24.0 * s,
        };
        if hit_box.contains(px, py) {
            sub_options.stroke_width = level;
            sub_options.custom_stroke_width = level.to_pixel();
            return;
        }
        cur_x += 24.0 * s;
    }

    cur_x += 16.0 * s;

    for color in preset_colors() {
        let hit_box = Rect {
            x: cur_x - 4.0 * s,
            y: center_y - 12.0 * s,
            width: 24.0 * s,
            height: 24.0 * s,
        };
        if hit_box.contains(px, py) {
            sub_options.color = color;
            return;
        }
        cur_x += 26.0 * s;
    }
}

/// 根据滚轮方向轮转切换粗细等级
pub(crate) fn cycle_stroke_width_level(
    current: StrokeWidthLevel,
    increase: bool,
) -> (StrokeWidthLevel, bool) {
    let next = if increase {
        match current {
            StrokeWidthLevel::Thin => StrokeWidthLevel::Medium,
            StrokeWidthLevel::Medium | StrokeWidthLevel::Thick => StrokeWidthLevel::Thick,
        }
    } else {
        match current {
            StrokeWidthLevel::Thick => StrokeWidthLevel::Medium,
            StrokeWidthLevel::Medium | StrokeWidthLevel::Thin => StrokeWidthLevel::Thin,
        }
    };

    let changed = next != current;
    (next, changed)
}
