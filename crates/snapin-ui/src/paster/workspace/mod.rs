//! 贴图子系统集中化单事件循环多窗口工作区调度模块。
//!
//! 负责由唯一的常驻 EventLoop 集中纳管多个独立的贴图窗口（WindowId），
//! 提供事件分发、分组显隐批量切换以及动态窗口生命周期管理，
//! 彻底消除每个贴图窗口独立创建进程与独立事件循环的反模式。

pub mod event_handler;
pub mod proxy;
pub mod window_builder;

pub use proxy::{PasterAppEvent, PasterWorkspaceProxy, WorkspaceStatus};
pub use window_builder::PasterInstance;

use super::manager::PasterManager;
use super::window::jobs::PasterJobExecutor;
use crate::error::UiError;
use event_handler::{
    handle_workspace_about_to_wait, handle_workspace_user_event, handle_workspace_window_event,
};
use snapin_core::config::Config;
use std::collections::HashMap;
use std::sync::Arc;
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
use winit::platform::wayland::EventLoopBuilderExtWayland;
use winit::window::WindowId;

/// 贴图工作区事件循环持有的全部长生命周期资源。
///
/// 集中封装窗口实例、分组管理器、全局配置与后台任务执行器，
/// 既减少事件处理函数的平铺参数，也明确执行器的唯一归属方。
pub(crate) struct WorkspaceRuntime {
    pub instances: HashMap<WindowId, PasterInstance>,
    pub manager: PasterManager,
    pub cfg: Arc<Config>,
    pub jobs: Arc<PasterJobExecutor>,
    /// 工作区事件循环的线程安全代理，供异步另存为等任务回传结果
    pub proxy: PasterWorkspaceProxy,
    /// 单次会话模式下最后一个贴图关闭后是否退出事件循环
    pub exit_on_all_closed: bool,
}

/// 创建集中多窗口贴图工作区事件循环与配套代理
pub fn create_paster_workspace(
) -> Result<(EventLoop<PasterAppEvent>, PasterWorkspaceProxy), UiError> {
    let mut builder = EventLoopBuilder::<PasterAppEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = builder.build().map_err(|e| UiError::Window {
        message: format!("创建集中多窗口贴图工作区事件循环失败: {e}"),
    })?;
    let proxy = PasterWorkspaceProxy::new(event_loop.create_proxy());
    Ok((event_loop, proxy))
}

/// 在专用 GUI 独立线程中拉起集中贴图工作区，并在事件循环就绪后返回全局线程安全代理
pub fn spawn_paster_workspace_thread(
    exit_on_all_closed: bool,
) -> Result<PasterWorkspaceProxy, UiError> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let tokio_handle = tokio::runtime::Handle::try_current().ok();
    std::thread::Builder::new()
        .name("snapin-paster-gui".to_string())
        .spawn(move || {
            let _guard = tokio_handle.as_ref().map(|h| h.enter());
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
        .map_err(|e| UiError::Window {
            message: format!("派生贴图工作区 GUI 线程失败: {e}"),
        })?;

    rx.recv().map_err(|_| UiError::Window {
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
) -> Result<(), UiError> {
    let mut manager = PasterManager::default();
    let cfg = Arc::new(snapin_core::config::load_or_create_config(None).unwrap_or_default());
    let initial_group = cfg
        .paster
        .active_group_id
        .min(event_handler::TOTAL_PIN_GROUPS.saturating_sub(1));
    manager.switch_group(initial_group);

    // 后台执行器由工作区独占持有：线程句柄不可丢弃，事件循环退出后必须 join。
    let jobs = Arc::new(PasterJobExecutor::try_new(
        event_loop.create_proxy(),
        cfg.clone(),
    )?);
    let proxy = PasterWorkspaceProxy::new(event_loop.create_proxy());
    let mut runtime = WorkspaceRuntime {
        instances: HashMap::new(),
        manager,
        cfg,
        jobs: jobs.clone(),
        proxy,
        exit_on_all_closed,
    };

    let run_result = event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);

        match event {
            Event::UserEvent(app_event) => {
                handle_workspace_user_event(app_event, &mut runtime, elwt);
            }
            Event::WindowEvent { window_id, event } => {
                handle_workspace_window_event(window_id, event, &mut runtime, elwt);
            }
            Event::AboutToWait => {
                let deadline = handle_workspace_about_to_wait(&mut runtime);
                elwt.set_control_flow(match deadline {
                    Some(deadline) => ControlFlow::WaitUntil(deadline),
                    None => ControlFlow::Wait,
                });
            }
            Event::LoopExiting => {
                tracing::info!("集中多窗口贴图工作区事件循环退出");
            }
            _ => {}
        }
    });

    // 无论事件循环正常退出还是异常返回，都必须停止并回收后台任务线程。
    jobs.shutdown();
    run_result.map_err(|e| UiError::Window {
        message: format!("运行集中贴图工作区事件主循环失败: {e}"),
    })
}
