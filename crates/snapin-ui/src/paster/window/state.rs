//! 贴图窗口运行时状态封装与窗口尺寸缩放管理。

use crate::paster::context_menu::ContextMenuState;
use crate::paster::manager::PasterItem;
use snapin_core::config::{Config, ZoomAnchor};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::window::Window;

/// 贴图窗口运行时状态
pub struct PasterWindowState {
    pub item: PasterItem,
    pub should_close: bool,
    pub has_shadow: bool,
    pub show_toolbar: bool,
    pub smooth_scaling: bool,
    pub file_path: Option<PathBuf>,
    pub context_menu: ContextMenuState,
    pub cursor_pos: (f32, f32),
    pub ui_scale: f32,
    pub config: Arc<Config>,
    pub(crate) last_click_time: Option<Instant>,
}

impl PasterWindowState {
    pub fn new(item: PasterItem) -> Self {
        Self {
            item,
            should_close: false,
            has_shadow: true,
            show_toolbar: false,
            smooth_scaling: true,
            file_path: None,
            context_menu: ContextMenuState::default(),
            cursor_pos: (0.0, 0.0),
            ui_scale: 1.0,
            config: Arc::new(Config::default()),
            last_click_time: None,
        }
    }

    /// 根据用户配置初始化贴图状态（注入初始不透明度）
    pub fn from_config(mut item: PasterItem, config: Arc<Config>) -> Self {
        item.transform.opacity = config.paster.initial_opacity.clamp(0.1, 1.0);
        let mut state = Self::new(item);
        state.config = config;
        state
    }

    /// 顺时针旋转 90 度并同步调整物理窗口尺寸
    pub fn rotate_cw(&mut self, window: &Window) -> bool {
        if let Some(rotated) = crate::paster::transform::rotate_pixmap_cw(&self.item.pixmap) {
            self.item.pixmap = rotated;
            apply_window_scale_resize(self, window);
            true
        } else {
            false
        }
    }

    /// 逆时针旋转 90 度并同步调整物理窗口尺寸
    pub fn rotate_ccw(&mut self, window: &Window) -> bool {
        if let Some(rotated) = crate::paster::transform::rotate_pixmap_ccw(&self.item.pixmap) {
            self.item.pixmap = rotated;
            apply_window_scale_resize(self, window);
            true
        } else {
            false
        }
    }

    /// 水平翻转图像
    pub fn flip_horizontal(&mut self) -> bool {
        if let Some(flipped) = crate::paster::transform::flip_pixmap_horizontal(&self.item.pixmap) {
            self.item.pixmap = flipped;
            true
        } else {
            false
        }
    }

    /// 垂直翻转图像
    pub fn flip_vertical(&mut self) -> bool {
        if let Some(flipped) = crate::paster::transform::flip_pixmap_vertical(&self.item.pixmap) {
            self.item.pixmap = flipped;
            true
        } else {
            false
        }
    }

    /// 设置缩放比例并同步调整物理窗口尺寸
    pub fn set_scale(&mut self, scale: f32, window: &Window) {
        self.item.transform.set_scale(scale);
        apply_window_scale_resize(self, window);
    }

    /// 滚轮平滑缩放，并按配置的光标/中心锚点重新定位窗口。
    pub fn zoom(&mut self, delta: f32, window: &Window) {
        let old_size = window.inner_size();
        let old_scale = self.item.transform.scale;
        self.item.transform.zoom(delta);
        let new_scale = self.item.transform.scale;
        if (new_scale - old_scale).abs() < f32::EPSILON {
            return;
        }

        let base_width = self.item.pixmap.width() as f32;
        let base_height = self.item.pixmap.height() as f32;
        let new_width = (base_width * new_scale).max(20.0);
        let new_height = (base_height * new_scale).max(20.0);
        let (ratio_x, ratio_y) = zoom_anchor_ratio(
            self.config.paster.zoom_anchor,
            self.cursor_pos,
            old_size.width as f32,
            old_size.height as f32,
        );

        if let Ok(position) = window.outer_position() {
            let delta_x = ((new_width - old_size.width as f32) * ratio_x).round() as i32;
            let delta_y = ((new_height - old_size.height as f32) * ratio_y).round() as i32;
            window.set_outer_position(PhysicalPosition::new(
                position.x - delta_x,
                position.y - delta_y,
            ));
        }

        apply_window_scale_resize(self, window);
    }

    /// 微调不透明度
    pub fn adjust_opacity(&mut self, delta: f32) {
        self.item.transform.adjust_opacity(delta);
    }

    /// 更新贴图像素内容并同步调整物理窗口尺寸
    pub fn replace_pixmap(&mut self, pixmap: tiny_skia::Pixmap, window: &Window) {
        self.item.pixmap = pixmap;
        apply_window_scale_resize(self, window);
    }
}

/// 重新计算并应用根据缩放比例与右键菜单展开状态调整后的窗口尺寸
pub(crate) fn apply_window_scale_resize(state: &PasterWindowState, window: &Window) {
    let base_w = state.item.pixmap.width() as f32;
    let base_h = state.item.pixmap.height() as f32;
    let scale = state.item.transform.scale;
    let mut new_w = (base_w * scale).max(20.0);
    let mut new_h = (base_h * scale).max(20.0);

    // 若当前正在展示右键菜单，确保窗口物理尺寸足以容纳菜单而不被截断
    if state.context_menu.is_open {
        let main_items = ContextMenuState::build_main_items(state.show_toolbar, state.has_shadow);
        let (req_w, req_h) = state.context_menu.required_menu_extent(
            state.cursor_pos.0,
            state.cursor_pos.1,
            &main_items,
        );
        new_w = new_w.max(req_w);
        new_h = new_h.max(req_h);
    }

    let _ = window.request_inner_size(PhysicalSize::new(new_w as u32, new_h as u32));
}

/// 计算缩放时用于保持锚点稳定的相对位置比例。
///
/// - 光标锚点：返回光标在旧窗口尺寸中的相对比例；
/// - 中心锚点：返回 `(0.5, 0.5)`。
pub(crate) fn zoom_anchor_ratio(
    anchor: ZoomAnchor,
    cursor: (f32, f32),
    old_width: f32,
    old_height: f32,
) -> (f32, f32) {
    match anchor {
        ZoomAnchor::Cursor => (
            (cursor.0 / old_width.max(1.0)).clamp(0.0, 1.0),
            (cursor.1 / old_height.max(1.0)).clamp(0.0, 1.0),
        ),
        ZoomAnchor::Center => (0.5, 0.5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_anchor_ratio() {
        assert_eq!(
            zoom_anchor_ratio(ZoomAnchor::Cursor, (100.0, 50.0), 400.0, 100.0),
            (0.25, 0.5)
        );
        assert_eq!(
            zoom_anchor_ratio(ZoomAnchor::Center, (100.0, 50.0), 400.0, 100.0),
            (0.5, 0.5)
        );
        assert_eq!(
            zoom_anchor_ratio(ZoomAnchor::Cursor, (900.0, 900.0), 400.0, 100.0),
            (1.0, 1.0)
        );
    }
}
