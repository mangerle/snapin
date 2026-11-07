//! 二级属性栏与属性控件绘制。

use super::super::items::{closest_preset_color_index, preset_colors};
use super::super::Toolbar;
use super::dropdown::render_dropdown;
use super::sub_bar_controls::SubControlRenderer;
use crate::geometry::Rect;
use tiny_skia::PixmapMut;

/// 绘制二级属性栏内容（形状、数值尺寸、线型、颜色与工具专属控件，自适应 HiDPI 缩放）
pub(super) fn render_sub_options(toolbar: &Toolbar, pixmap: &mut PixmapMut, rect: &Rect) {
    let Some(layout) = toolbar.sub_layout() else {
        return;
    };
    let s = toolbar.ui_scale;
    let center_y = rect.y + rect.height / 2.0;
    let color = toolbar.sub_options.color;
    let highlight = tiny_skia::Color::from_rgba8(37, 99, 235, 45);
    let presets = preset_colors();
    let selected_color_index = closest_preset_color_index(color);

    for entry in &layout.entries {
        SubControlRenderer {
            toolbar,
            pixmap,
            rect: entry.rect,
            center_y,
            scale: s,
            color,
            highlight,
            presets,
            selected_color_index,
        }
        .render(entry.control);
    }

    if let Some(dropdown) = &layout.dropdown {
        render_dropdown(pixmap, dropdown, toolbar, s);
    }
}
