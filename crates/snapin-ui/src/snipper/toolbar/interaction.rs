//! 截图工具栏鼠标交互与事件分发。
//!
//! 负责工具栏各按钮的鼠标悬停碰撞检测、点击工具切换、
//! 二级属性栏颜色与线宽调整，以及滚轮快捷阶梯切换线宽。

use crate::snipper::selection::Rect;
use crate::snipper::toolbar::items::ToolbarItem;
use crate::snipper::toolbar::metrics::{
    button_size, divider_width, has_separator_before, padding, spacing,
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
    let btn_size = button_size(s);
    let padding = padding(s);
    let spacing = spacing(s);
    let mut cur_x = main_rect.x + padding;

    for (idx, &item) in items.iter().enumerate() {
        if has_separator_before(items, idx) {
            cur_x += divider_width(s);
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
