//! 截图工具栏鼠标交互与事件分发。
//!
//! 负责工具栏各按钮的鼠标悬停碰撞检测、点击工具切换、
//! 二级属性栏颜色与线宽调整，以及滚轮快捷阶梯切换线宽。

use crate::editor::toolbar::items::ToolbarItem;
use crate::editor::toolbar::metrics::{
    button_size, divider_width, has_separator_before, padding, spacing,
};
use crate::geometry::Rect;

/// 计算主栏每个按钮的矩形（含分隔线占位）。
///
/// 命中测试、悬浮高亮与悬浮提示都依赖同一份几何，集中于此避免三处各自累加
/// 内边距、按钮尺寸、间距与分隔线宽度而产生像素级漂移。
pub(crate) fn main_item_rects(
    main_rect: &Rect,
    items: &[ToolbarItem],
    ui_scale: f32,
) -> Vec<(ToolbarItem, Rect)> {
    let s = ui_scale;
    let btn_size = button_size(s);
    let padding = padding(s);
    let spacing = spacing(s);
    let mut cur_x = main_rect.x + padding;
    let mut rects = Vec::with_capacity(items.len());

    for (idx, &item) in items.iter().enumerate() {
        if has_separator_before(items, idx) {
            cur_x += divider_width(s);
        }
        rects.push((
            item,
            Rect {
                x: cur_x,
                y: main_rect.y + padding,
                width: btn_size,
                height: btn_size,
            },
        ));
        cur_x += btn_size + spacing;
    }

    rects
}

/// 主工具栏按钮命中测试参数。
#[derive(Debug, Clone, Copy)]
pub(crate) struct MainHitTestRequest<'a> {
    pub main_rect: &'a Rect,
    pub items: &'a [ToolbarItem],
    pub ui_scale: f32,
    pub point: (f32, f32),
}

/// 主工具栏按钮命中测试 (计算每个按钮的热区并进行点碰撞检测)
pub(crate) fn hit_test_main_items(request: MainHitTestRequest<'_>) -> Option<ToolbarItem> {
    if !request.main_rect.contains(request.point.0, request.point.1) {
        return None;
    }

    main_item_rects(request.main_rect, request.items, request.ui_scale)
        .into_iter()
        .find_map(|(item, rect)| {
            rect.contains(request.point.0, request.point.1)
                .then_some(item)
        })
}
