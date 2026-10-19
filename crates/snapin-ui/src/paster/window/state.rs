//! 贴图窗口运行时状态封装与窗口尺寸缩放管理。

use crate::paster::context_menu::ContextMenuState;
use crate::paster::manager::PasterItem;
use snapin_core::config::Config;
use std::path::PathBuf;
use std::time::Instant;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// 贴图窗口运行时状态
pub struct PasterWindowState {
    pub item: PasterItem,
    pub should_close: bool,
    pub should_destroy: bool,
    pub has_shadow: bool,
    pub show_toolbar: bool,
    pub smooth_scaling: bool,
    pub file_path: Option<PathBuf>,
    pub context_menu: ContextMenuState,
    pub cursor_pos: (f32, f32),
    pub ui_scale: f32,
    pub config: Config,
    pub(crate) last_click_time: Option<Instant>,
}

impl PasterWindowState {
    pub fn new(item: PasterItem) -> Self {
        Self {
            item,
            should_close: false,
            should_destroy: false,
            has_shadow: true,
            show_toolbar: false,
            smooth_scaling: true,
            file_path: None,
            context_menu: ContextMenuState::default(),
            cursor_pos: (0.0, 0.0),
            ui_scale: 1.0,
            config: Config::default(),
            last_click_time: None,
        }
    }

    /// 根据用户配置初始化贴图状态（注入初始不透明度）
    pub fn from_config(mut item: PasterItem, cfg: Config) -> Self {
        item.transform.opacity = cfg.paster.initial_opacity.clamp(0.1, 1.0);
        let mut state = Self::new(item);
        state.config = cfg;
        state
    }
}

/// 重新计算并应用根据缩放比例调整后的窗口尺寸
pub(crate) fn apply_window_scale_resize(state: &PasterWindowState, window: &Window) {
    let base_w = state.item.pixmap.width() as f32;
    let base_h = state.item.pixmap.height() as f32;
    let scale = state.item.transform.scale;
    let new_w = (base_w * scale).max(20.0) as u32;
    let new_h = (base_h * scale).max(20.0) as u32;
    let _ = window.request_inner_size(PhysicalSize::new(new_w, new_h));
}
