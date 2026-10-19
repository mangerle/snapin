//! 贴图窗口物理实例与图形表面构建。

use super::super::manager::{PasterItem, PasterManager};
use super::super::window::events::PasterWindowState;
use super::proxy::PasterAppEvent;
use snapin_core::config::Config;
use snapin_core::error::SnapinError;
use snapin_platform::pin_window_above;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::EventLoopWindowTarget;
use winit::platform::wayland::WindowBuilderExtWayland;
use winit::window::{Window, WindowBuilder, WindowId};

/// 单个受管贴图窗口的运行时物理实例
pub struct PasterInstance {
    pub item_id: u64,
    pub group_id: usize,
    pub window: Arc<Window>,
    pub _context: softbuffer::Context<Arc<Window>>,
    pub surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pub state: PasterWindowState,
}

/// 在当前集中事件循环中动态创建并注入一个新贴图子窗口
pub fn create_new_paster_window(
    item: PasterItem,
    instances: &mut HashMap<WindowId, PasterInstance>,
    manager: &mut PasterManager,
    cfg: &Config,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) -> Result<(), SnapinError> {
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
            .map_err(|e| SnapinError::WindowError {
                message: format!("创建集中工作区贴图窗口失败: {e}"),
            })?,
    );

    let context =
        softbuffer::Context::new(window.clone()).map_err(|e| SnapinError::SoftbufferError {
            message: format!("初始化贴图软渲染上下文失败: {e}"),
        })?;

    let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| {
        SnapinError::SoftbufferError {
            message: format!("创建贴图软渲染帧缓冲表面失败: {e}"),
        }
    })?;

    // 自动置顶与 GNOME 扩展协调
    if cfg.paster.enable_above_extension {
        let t_clone = title.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = pin_window_above(&t_clone).await;
        });
    }

    let scale_factor = window.scale_factor() as f32;
    let auto_scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };

    let mut state = PasterWindowState::from_config(item.clone(), cfg.clone());
    state.ui_scale = auto_scale;
    state.context_menu.set_ui_scale(auto_scale);

    let item_id = item.id;
    let group_id = item.group_id;
    let win_id = window.id();

    manager.add_item(item.clone());
    window.request_redraw();

    let instance = PasterInstance {
        item_id,
        group_id,
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

    Ok(())
}
