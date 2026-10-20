//! 截图工具栏统一布局度量与分隔位置定义。
//!
//! 布局计算、命中测试与光栅化渲染共用同一组常量，避免三处硬编码漂移。

/// 主工具栏中需要前置分隔线的按钮下标
pub(crate) const SEPARATOR_BEFORE_INDICES: &[usize] = &[7, 11];

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

/// 指定按钮下标前是否需要绘制分隔线
pub(crate) fn has_separator_before(index: usize) -> bool {
    SEPARATOR_BEFORE_INDICES.contains(&index)
}

/// 计算主工具栏总宽度
pub(crate) fn main_toolbar_width(item_count: usize, scale: f32) -> f32 {
    let items = item_count as f32;
    let separators = SEPARATOR_BEFORE_INDICES.len() as f32;
    padding(scale) * 2.0
        + items * button_size(scale)
        + (items - 1.0).max(0.0) * spacing(scale)
        + separators * divider_width(scale)
}
