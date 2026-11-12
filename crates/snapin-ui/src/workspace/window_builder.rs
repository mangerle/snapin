//! 贴图窗口物理实例与图形表面构建。

use super::WorkspaceRuntime;
use crate::error::UiError;
use crate::model::PasterId;
use crate::model::PasterItem;
use crate::paster::surface::PasterSurface;
use crate::paster::window::events::PasterWindowState;
use crate::paster::MAX_PIN_WINDOW_DIM;
use std::sync::Arc;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowLevel};

/// 单个受管贴图窗口的运行时物理实例
pub struct PasterInstance {
    pub item_id: PasterId,
    pub window: Arc<Window>,
    pub surface: PasterSurface,
    pub state: PasterWindowState,
    /// 当前连续渲染失败是否已经上报，避免重绘循环反复刷日志。
    pub render_error_reported: bool,
}

/// 在当前集中事件循环中动态创建并注入一个新贴图子窗口
pub(crate) fn create_new_paster_window(
    mut item: PasterItem,
    runtime: &mut WorkspaceRuntime,
    elwt: &ActiveEventLoop,
) -> Result<(), UiError> {
    // 新贴图始终归属当前活动分组，避免分组切换后出现“管理器隐藏但窗口仍显示”的分裂状态。
    item.group_id = runtime.manager.active_group();
    let initial_w = item.pixmap.width();
    let initial_h = item.pixmap.height();
    let initial_size = clamp_initial_paster_size(&mut item, elwt);
    let title = item.title.clone();
    let window = create_window(&item, initial_size, elwt)?;

    // 图形表面优先走支持透明像素的 ARGB8888 共享内存通路，失败时内部回退 softbuffer
    let surface = PasterSurface::new(window.clone())?;
    let item_id = item.id;
    let group_id = item.group_id;
    let state = build_window_state(item, runtime, window.scale_factor() as f32);

    let win_id = window.id();
    if !runtime.manager.add_entry(item_id, group_id) {
        return Err(UiError::Window {
            message: format!("贴图标识重复，拒绝创建窗口: {item_id:?}"),
        });
    }
    window.request_redraw();

    let instance = PasterInstance {
        item_id,
        window,
        surface,
        state,
        render_error_reported: false,
    };

    runtime.instances.insert(win_id, instance);
    tracing::info!(
        "已成功在集中工作区创建贴图: {title} ({initial_w}x{initial_h}, 窗口 {}x{})，工作区活跃窗口数: {}",
        initial_size.0,
        initial_size.1,
        runtime.instances.len()
    );

    Ok(())
}

/// 计算并夹紧新贴图窗口的初始物理尺寸，同时回写贴图初始显示倍率。
///
/// 无论贴图来源是普通截屏、长截图还是导入，都确保底层物理窗口与帧缓冲不超出当前所在显示器
/// 视口安全阈值以及图形表面单边上限，防止创建超大透明窗口引发 Wayland/X11/GPU 崩溃。
fn clamp_initial_paster_size(item: &mut PasterItem, elwt: &ActiveEventLoop) -> (u32, u32) {
    let (mon_w, mon_h) = target_monitor_dimensions(item.position, elwt);
    let max_w = ((mon_w as f32) * 0.90).clamp(20.0, MAX_PIN_WINDOW_DIM as f32);
    let max_h = ((mon_h as f32) * 0.90).clamp(20.0, MAX_PIN_WINDOW_DIM as f32);
    let wf = item.pixmap.width().max(1) as f32;
    let hf = item.pixmap.height().max(1) as f32;
    let fit_scale = (max_w / wf).min(max_h / hf);
    let clamped_scale = item.transform.scale().min(fit_scale).clamp(0.1, 10.0);
    if clamped_scale < item.transform.scale() {
        item.transform.set_scale(clamped_scale);
    }
    let scale = item.transform.scale();
    let win_w = ((wf * scale).round() as u32).clamp(20, MAX_PIN_WINDOW_DIM);
    let win_h = ((hf * scale).round() as u32).clamp(20, MAX_PIN_WINDOW_DIM);
    (win_w, win_h)
}

/// 查询贴图落点所属显示器的物理像素尺寸；无可用信息时回退常规 1080p 边界。
fn target_monitor_dimensions(position: (i32, i32), elwt: &ActiveEventLoop) -> (u32, u32) {
    let monitor = elwt
        .available_monitors()
        .find(|m| {
            let pos = m.position();
            let size = m.size();
            let right = pos.x.saturating_add(size.width as i32);
            let bottom = pos.y.saturating_add(size.height as i32);
            position.0 >= pos.x && position.0 < right && position.1 >= pos.y && position.1 < bottom
        })
        .or_else(|| elwt.primary_monitor())
        .or_else(|| elwt.available_monitors().next());
    match monitor {
        Some(m) => {
            let size = m.size();
            if size.width > 0 && size.height > 0 {
                (size.width, size.height)
            } else {
                (1920, 1080)
            }
        }
        None => (1920, 1080),
    }
}

/// 创建贴图子窗口的物理属性。
fn window_attributes(item: &PasterItem, initial_size: (u32, u32)) -> WindowAttributes {
    let attributes = Window::default_attributes()
        .with_title(&item.title)
        .with_decorations(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(PhysicalSize::new(initial_size.0, initial_size.1))
        .with_position(PhysicalPosition::new(item.position.0, item.position.1));

    #[cfg(target_os = "linux")]
    {
        use winit::platform::wayland::WindowAttributesExtWayland;
        attributes
            .with_transparent(true)
            .with_name("snapin-pin", "snapin-pin")
    }
    #[cfg(not(target_os = "linux"))]
    {
        attributes
    }
}

/// 在当前事件循环中创建贴图窗口。
fn create_window(
    item: &PasterItem,
    initial_size: (u32, u32),
    elwt: &ActiveEventLoop,
) -> Result<Arc<Window>, UiError> {
    elwt.create_window(window_attributes(item, initial_size))
        .map(Arc::new)
        .map_err(|error| UiError::Window {
            message: format!("创建集中工作区贴图窗口失败: {error}"),
        })
}

/// 构建窗口交互状态并注入工作区共享端口。
fn build_window_state(
    item: PasterItem,
    runtime: &WorkspaceRuntime,
    scale_factor: f32,
) -> PasterWindowState {
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let mut state = PasterWindowState::from_config(item, runtime.cfg.clone());
    state.viewport.ui_scale = scale;
    state.interaction.context_menu.set_ui_scale(scale);
    // 工作区级端口注入窗口状态后，工具栏动作才能把整图任务投递到应用层执行器
    state.services.job_runtime = Some(runtime.jobs.clone());
    state.services.desktop = runtime.desktop.clone();
    state.services.clipboard_events = runtime.clipboard_events.clone();
    state
}
