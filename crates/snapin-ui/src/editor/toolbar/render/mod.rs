//! 工具栏与二级属性栏的纯 tiny-skia 矢量光栅化绘制。
//!
//! 渲染按主栏、属性栏、下拉菜单与共享原语拆分，避免单文件持续膨胀；
//! 各子模块只读取 [`Toolbar`] 的布局结果，不修改状态。

mod dropdown;
mod magnifier_controls;
mod main_bar;
mod sub_bar;
mod sub_bar_controls;
mod sub_bar_helpers;
mod sub_bar_icons;
mod sub_bar_sliders;
mod sub_bar_text_controls;
mod tooltip;
mod widgets;

use super::Toolbar;
use main_bar::{render_drag_handle, render_main_buttons};
use sub_bar::render_sub_options;
use tiny_skia::PixmapMut;
use tooltip::render_hover_tooltip;
use widgets::render_panel_background;

/// 渲染整个工具栏界面 (包含主工具栏与二级属性栏)
pub fn render_toolbar(toolbar: &Toolbar, pixmap: &mut PixmapMut) {
    if toolbar.main_rect.width <= 0.0 || toolbar.main_rect.height <= 0.0 {
        return;
    }
    let s = toolbar.ui_scale;

    // 1. 绘制主工具栏面板背景
    render_panel_background(pixmap, &toolbar.main_rect, s);

    // 2. 绘制各个主按钮与矢量图标
    render_main_buttons(toolbar, pixmap);
    if toolbar.draggable {
        render_drag_handle(pixmap, &toolbar.main_rect, s);
    }

    // 3. 绘制二级属性栏 (若激活)
    if let Some(sub_rect) = toolbar.sub_rect {
        render_panel_background(pixmap, &sub_rect, s);
        render_sub_options(toolbar, pixmap, &sub_rect);
    }

    // 4. 悬浮提示最后绘制，保证位于主栏、属性栏与下拉菜单之上
    render_hover_tooltip(toolbar, pixmap);
}

impl Toolbar {
    /// 使用 tiny-skia 将工具栏绘制到目标 Pixmap
    pub fn render(&self, pixmap: &mut PixmapMut) {
        render_toolbar(self, pixmap);
    }
}
