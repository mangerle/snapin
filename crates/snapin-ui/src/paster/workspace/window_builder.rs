//! 贴图窗口物理实例与图形表面构建。

use super::super::manager::{PasterItem, PasterManager};
use super::super::window::events::PasterWindowState;
use super::proxy::PasterAppEvent;
use crate::error::UiError;
use snapin_core::config::Config;
use std::collections::HashMap;
use std::sync::Arc;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::EventLoopWindowTarget;
use winit::platform::wayland::WindowBuilderExtWayland;
use winit::window::{Window, WindowBuilder, WindowId};

/// 单个受管贴图窗口的运行时物理实例
pub struct PasterInstance {
    pub item_id: u64,
    pub window: Arc<Window>,
    pub _context: softbuffer::Context<Arc<Window>>,
    pub surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pub state: PasterWindowState,
}

/// 在当前集中事件循环中动态创建并注入一个新贴图子窗口
pub fn create_new_paster_window(
    mut item: PasterItem,
    instances: &mut HashMap<WindowId, PasterInstance>,
    manager: &mut PasterManager,
    cfg: &Arc<Config>,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) -> Result<(), UiError> {
    // 新贴图始终归属当前活动分组，避免分组切换后出现“管理器隐藏但窗口仍显示”的分裂状态。
    item.group_id = manager.active_group();
    let title = item.title.clone();
    let initial_w = item.pixmap.width();
    let initial_h = item.pixmap.height();
    let (init_x, init_y) = item.position;

    let window = Arc::new(
        WindowBuilder::new()
            .with_name("snapin-pin", "snapin-pin")
            .with_title(&title)
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size(PhysicalSize::new(initial_w, initial_h))
            .with_position(PhysicalPosition::new(init_x, init_y))
            .build(elwt)
            .map_err(|e| UiError::Window {
                message: format!("创建集中工作区贴图窗口失败: {e}"),
            })?,
    );

    let context = softbuffer::Context::new(window.clone()).map_err(|e| UiError::Softbuffer {
        message: format!("初始化贴图软渲染上下文失败: {e}"),
    })?;

    let surface =
        softbuffer::Surface::new(&context, window.clone()).map_err(|e| UiError::Softbuffer {
            message: format!("创建贴图软渲染帧缓冲表面失败: {e}"),
        })?;

    let scale_factor = window.scale_factor() as f32;
    let auto_scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };

    let item_id = item.id;
    let group_id = item.group_id;
    let mut state = PasterWindowState::from_config(item, cfg.clone());
    state.ui_scale = auto_scale;
    state.context_menu.set_ui_scale(auto_scale);

    let win_id = window.id();
    manager.add_entry(item_id, group_id);
    window.request_redraw();

    let instance = PasterInstance {
        item_id,
        window,
        _context: context,
        surface,
        state,
    };

    instances.insert(win_id, instance);
    tracing::info!(
        "已成功在集中工作区创建贴图: {title} ({initial_w}x{initial_h})，工作区活跃窗口数: {}",
        instances.len()
    );

    // 通知贴图生命周期观察者（由外部注入 GNOME 扩展置顶等平台行为，解耦 UI 表现层与平台特权）
    if let Some(observer) = super::get_paster_observer() {
        observer.on_paster_created(&title, win_id);
    }

    Ok(())
}
