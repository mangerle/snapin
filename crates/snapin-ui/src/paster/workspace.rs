//! 贴图子系统集中化单事件循环多窗口工作区调度模块。
//!
//! 负责由唯一的常驻 EventLoop 集中纳管多个独立的贴图窗口（WindowId），
//! 提供事件分发、分组显隐批量切换以及动态窗口生命周期管理，
//! 彻底消除每个贴图窗口独立创建进程与独立事件循环的反模式。

use super::manager::{PasterItem, PasterManager};
use super::window::events::{
    handle_paster_cursor_move, handle_paster_key, handle_paster_mouse_button, handle_paster_scroll,
    PasterWindowState,
};
use super::window::render::render_paster_to_surface;
use snapin_core::config::Config;
use snapin_core::error::SnapinError;
use snapin_platform::pin_window_above;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{
    ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget,
};
use winit::platform::wayland::{EventLoopBuilderExtWayland, WindowBuilderExtWayland};
use winit::window::{Window, WindowBuilder, WindowId};

/// 工作区当前状态快照
#[derive(Debug, Clone)]
pub struct WorkspaceStatus {
    pub active_group: usize,
    pub paster_count: usize,
}

/// 集中贴图工作区支持的用户事件指令
pub enum PasterAppEvent {
    /// 创建并弹出一个新的贴图窗口
    OpenPaster(PasterItem),
    /// 批量切换当前分组内所有贴图的显隐状态
    ToggleVisibility,
    /// 切换活动分组（若为 None 则循环轮转至下一个分组）
    SwitchGroup(Option<usize>),
    /// 显式设置当前活动组所有贴图的可见性
    SetVisibility(bool),
    /// 关闭指定 ID 的贴图
    ClosePaster(u64),
    /// 查询当前工作区运行状态
    GetStatus(oneshot::Sender<WorkspaceStatus>),
    /// 请求工作区平滑停机
    Shutdown,
}

/// 贴图工作区线程安全代理句柄，支持跨线程、跨任务异步发送控制事件
#[derive(Clone)]
pub struct PasterWorkspaceProxy {
    proxy: EventLoopProxy<PasterAppEvent>,
}

impl PasterWorkspaceProxy {
    /// 封装 winit 事件循环代理
    pub fn new(proxy: EventLoopProxy<PasterAppEvent>) -> Self {
        Self { proxy }
    }

    /// 请求工作区打开新贴图窗口
    pub fn open_paster(&self, item: PasterItem) -> Result<(), SnapinError> {
        self.proxy
            .send_event(PasterAppEvent::OpenPaster(item))
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递打开窗口指令失败: {e}"),
            })
    }

    /// 请求批量切换当前组贴图显隐
    pub fn toggle_visibility(&self) -> Result<(), SnapinError> {
        self.proxy
            .send_event(PasterAppEvent::ToggleVisibility)
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递显隐切换指令失败: {e}"),
            })
    }

    /// 请求切换活动贴图分组
    pub fn switch_group(&self, group_id: Option<usize>) -> Result<(), SnapinError> {
        self.proxy
            .send_event(PasterAppEvent::SwitchGroup(group_id))
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递分组切换指令失败: {e}"),
            })
    }

    /// 异步获取工作区当前活动状态快照
    pub async fn get_status(&self) -> Result<WorkspaceStatus, SnapinError> {
        let (tx, rx) = oneshot::channel();
        self.proxy
            .send_event(PasterAppEvent::GetStatus(tx))
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递状态查询指令失败: {e}"),
            })?;
        rx.await.map_err(|_| SnapinError::WindowError {
            message: "等待贴图工作区响应状态快照超时或主循环已退出".to_string(),
        })
    }

    /// 请求工作区平滑退出
    pub fn shutdown(&self) -> Result<(), SnapinError> {
        self.proxy
            .send_event(PasterAppEvent::Shutdown)
            .map_err(|e| SnapinError::WindowError {
                message: format!("向贴图工作区投递停机指令失败: {e}"),
            })
    }
}

/// 单个受管贴图窗口的运行时物理实例
struct PasterInstance {
    item_id: u64,
    group_id: usize,
    window: Arc<Window>,
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    state: PasterWindowState,
}

/// 创建集中多窗口贴图工作区事件循环与配套代理
pub fn create_paster_workspace(
) -> Result<(EventLoop<PasterAppEvent>, PasterWorkspaceProxy), SnapinError> {
    let mut builder = EventLoopBuilder::<PasterAppEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build().map_err(|e| SnapinError::WindowError {
        message: format!("创建集中多窗口贴图工作区事件循环失败: {e}"),
    })?;
    let proxy = PasterWorkspaceProxy::new(event_loop.create_proxy());
    Ok((event_loop, proxy))
}

/// 在专用 GUI 独立线程中拉起集中贴图工作区，并在事件循环就绪后返回全局线程安全代理
pub fn spawn_paster_workspace_thread(
    exit_on_all_closed: bool,
) -> Result<PasterWorkspaceProxy, SnapinError> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("snapin-paster-gui".to_string())
        .spawn(move || {
            let (event_loop, proxy) = match create_paster_workspace() {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::error!("创建集中贴图工作区失败: {e}");
                    return;
                }
            };
            let _ = tx.send(proxy);
            if let Err(e) = run_paster_workspace_loop(event_loop, exit_on_all_closed) {
                tracing::error!("集中贴图工作区主循环异常退出: {e}");
            }
        })
        .map_err(|e| SnapinError::WindowError {
            message: format!("派生贴图工作区 GUI 线程失败: {e}"),
        })?;

    rx.recv().map_err(|_| SnapinError::WindowError {
        message: "等待集中贴图工作区事件循环就绪失败".to_string(),
    })
}

/// 运行集中贴图工作区事件主循环
///
/// 当 `exit_on_all_closed` 为 false 时（常驻守护模式），即便当前贴图全部关闭，事件循环也始终常驻等待；
/// 当 `exit_on_all_closed` 为 true 时（单次会话模式），在最后一个贴图关闭后自动退出事件循环。
pub fn run_paster_workspace_loop(
    event_loop: EventLoop<PasterAppEvent>,
    exit_on_all_closed: bool,
) -> Result<(), SnapinError> {
    let mut instances: HashMap<WindowId, PasterInstance> = HashMap::new();
    let mut manager = PasterManager::default();
    let cfg = snapin_core::config::load_or_create_config(None).unwrap_or_default();

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::UserEvent(app_event) => {
                    handle_workspace_user_event(
                        app_event,
                        &mut instances,
                        &mut manager,
                        &cfg,
                        elwt,
                    );
                }
                Event::WindowEvent { window_id, event } => {
                    handle_workspace_window_event(
                        window_id,
                        event,
                        &mut instances,
                        &mut manager,
                        exit_on_all_closed,
                        elwt,
                    );
                }
                Event::LoopExiting => {
                    tracing::info!("集中多窗口贴图工作区事件循环退出");
                }
                _ => {}
            }
        })
        .map_err(|e| SnapinError::WindowError {
            message: format!("运行集中贴图工作区事件主循环失败: {e}"),
        })
}

/// 处理工作区收到的自定义指令
fn handle_workspace_user_event(
    event: PasterAppEvent,
    instances: &mut HashMap<WindowId, PasterInstance>,
    manager: &mut PasterManager,
    cfg: &Config,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    match event {
        PasterAppEvent::OpenPaster(item) => {
            if let Err(e) = create_new_paster_window(item, instances, manager, cfg, elwt) {
                tracing::error!("在当前事件循环中创建新贴图窗口失败: {e}");
            }
        }
        PasterAppEvent::ToggleVisibility => {
            let new_visible = manager.toggle_current_group_visibility();
            let active = manager.active_group();
            for inst in instances.values_mut() {
                if inst.group_id == active {
                    inst.state.item.is_visible = new_visible;
                    inst.window.set_visible(new_visible);
                }
            }
            tracing::info!(
                "已切换当前贴图组 ({active}) 显隐状态: {}",
                if new_visible {
                    "全部显示"
                } else {
                    "全部隐藏"
                }
            );
        }
        PasterAppEvent::SwitchGroup(target) => {
            let next_group = match target {
                Some(id) => id,
                None => (manager.active_group() + 1) % 4,
            };
            manager.switch_group(next_group);
            for inst in instances.values_mut() {
                let visible = inst.group_id == next_group;
                inst.state.item.is_visible = visible;
                inst.window.set_visible(visible);
            }
            tracing::info!("已切换活动贴图分组至: {next_group}");
        }
        PasterAppEvent::SetVisibility(visible) => {
            let active = manager.active_group();
            for inst in instances.values_mut() {
                if inst.group_id == active {
                    inst.state.item.is_visible = visible;
                    inst.window.set_visible(visible);
                }
            }
        }
        PasterAppEvent::ClosePaster(id) => {
            if let Some((&win_id, _)) = instances.iter().find(|(_, inst)| inst.item_id == id) {
                if let Some(removed) = instances.remove(&win_id) {
                    manager.remove_paster(removed.item_id);
                    tracing::info!("已通过指令关闭贴图: {id}");
                }
            }
        }
        PasterAppEvent::GetStatus(responder) => {
            let status = WorkspaceStatus {
                active_group: manager.active_group(),
                paster_count: manager.count_in_active_group(),
            };
            let _ = responder.send(status);
        }
        PasterAppEvent::Shutdown => {
            instances.clear();
            elwt.exit();
        }
    }
}

/// 在当前集中事件循环中动态创建并注入一个新贴图子窗口
fn create_new_paster_window(
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
                message: format!("在集中事件循环中构建贴图子窗口失败: {e}"),
            })?,
    );

    let context =
        softbuffer::Context::new(window.clone()).map_err(|e| SnapinError::SoftbufferError {
            message: e.to_string(),
        })?;
    let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| {
        SnapinError::SoftbufferError {
            message: e.to_string(),
        }
    })?;

    // 触发配套 GNOME 扩展将本子窗口强制置顶
    if cfg.paster.enable_above_extension {
        let title_clone = title.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            if let Err(e) = pin_window_above(&title_clone).await {
                tracing::warn!("调用 GNOME 扩展置顶窗口失败: {e}");
            }
        });
    }

    let raw_scale = window.scale_factor() as f32;
    let auto_scale = if raw_scale > 1.05 { raw_scale } else { 1.0 };
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
        "已在集中工作区中挂载贴图窗口: {title}，当前受管贴图总数: {}",
        instances.len()
    );

    Ok(())
}

/// 分发具体窗口的输入事件与局部重绘
fn handle_workspace_window_event(
    window_id: WindowId,
    event: WindowEvent,
    instances: &mut HashMap<WindowId, PasterInstance>,
    manager: &mut PasterManager,
    exit_on_all_closed: bool,
    elwt: &EventLoopWindowTarget<PasterAppEvent>,
) {
    let Some(inst) = instances.get_mut(&window_id) else {
        return;
    };

    let mut should_close = false;

    match event {
        WindowEvent::RedrawRequested => {
            let size = inst.window.inner_size();
            if let Err(e) = render_paster_to_surface(
                &mut inst.state,
                &mut inst.surface,
                size.width,
                size.height,
            ) {
                tracing::error!("贴图渲染异常: {e}");
            }
        }
        WindowEvent::CursorMoved { position, .. } => {
            if handle_paster_cursor_move(&mut inst.state, (position.x, position.y)) {
                inst.window.request_redraw();
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            if handle_paster_scroll(&mut inst.state, delta, &inst.window) {
                inst.window.request_redraw();
            }
        }
        WindowEvent::MouseInput {
            state: btn_state,
            button,
            ..
        } => {
            if handle_paster_mouse_button(&mut inst.state, button, btn_state, &inst.window) {
                inst.window.request_redraw();
            }
            if inst.state.should_close {
                should_close = true;
            }
        }
        WindowEvent::KeyboardInput {
            event: key_event, ..
        } => {
            if handle_paster_key(
                &mut inst.state,
                &key_event.logical_key,
                key_event.state,
                &inst.window,
            ) {
                inst.window.request_redraw();
            }
            if inst.state.should_close {
                should_close = true;
            }
        }
        WindowEvent::CloseRequested => {
            should_close = true;
        }
        _ => {}
    }

    if should_close {
        if let Some(removed) = instances.remove(&window_id) {
            manager.remove_paster(removed.item_id);
            tracing::info!(
                "贴图已安全关闭: {}，工作区剩余活动贴图: {}",
                removed.item_id,
                instances.len()
            );
        }
        if exit_on_all_closed && instances.is_empty() {
            tracing::info!("所有单次贴图已关闭，工作区事件循环自动退场");
            elwt.exit();
        }
    }
}
