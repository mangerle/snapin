//! 截图工具栏统一布局度量与分隔位置定义。
//!
//! 布局计算、命中测试与光栅化渲染共用同一组常量，避免三处硬编码漂移。

use super::items::ToolbarItem;

/// 按钮边长
pub(crate) fn button_size(scale: f32) -> f32 {
    32.0 * scale
}

/// 面板内边距
pub(crate) fn padding(scale: f32) -> f32 {
    6.0 * scale
}

/// 按钮间距
pub(crate) fn spacing(scale: f32) -> f32 {
    4.0 * scale
}

/// 分隔线占用宽度
pub(crate) fn divider_width(scale: f32) -> f32 {
    8.0 * scale
}

/// 指定按钮前是否需要绘制分隔线。
///
/// 分隔位置直接由按钮语义决定（撤销/重做组与关闭动作之前），
/// 这样截图的完整按钮集与贴图的精简按钮集都能得到正确分隔，
/// 无需再维护一份与按钮序列强耦合的下标表。
pub(crate) fn has_separator_before(items: &[ToolbarItem], index: usize) -> bool {
    index > 0
        && matches!(
            items.get(index),
            Some(ToolbarItem::ActionUndo | ToolbarItem::ActionClose)
        )
}

/// 统计实际生效的分隔线数量（下标 0 之前不绘制分隔线）
fn separator_count(items: &[ToolbarItem]) -> usize {
    items
        .iter()
        .enumerate()
        .filter(|(index, item)| {
            *index > 0 && matches!(item, ToolbarItem::ActionUndo | ToolbarItem::ActionClose)
        })
        .count()
}

/// 计算主工具栏总宽度
pub(crate) fn main_toolbar_width(items: &[ToolbarItem], scale: f32) -> f32 {
    let count = items.len() as f32;
    padding(scale) * 2.0
        + count * button_size(scale)
        + (count - 1.0).max(0.0) * spacing(scale)
        + separator_count(items) as f32 * divider_width(scale)
}
