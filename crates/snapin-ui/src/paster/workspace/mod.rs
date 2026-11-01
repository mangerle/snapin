//! 贴图子系统集中化单事件循环多窗口工作区调度模块。
//!
//! 负责由唯一的常驻 EventLoop 集中纳管多个独立的贴图窗口（WindowId），
//! 提供事件分发、分组显隐批量切换以及动态窗口生命周期管理，
//! 彻底消除每个贴图窗口独立创建进程与独立事件循环的反模式。
//!
//! 工作区只消费宿主注入的端口与配置快照，不自行读取配置文件、不创建后台线程；
//! GUI 线程的派生与后台执行器生命周期属于应用层职责。

pub mod event_handler;
pub mod proxy;
pub mod window_builder;

pub use event_handler::{is_valid_group_id, TOTAL_PIN_GROUPS};
pub use proxy::{PasterAppEvent, PasterWorkspaceProxy, WorkspaceStatus};
pub use window_builder::PasterInstance;

use super::manager::PasterManager;
use super::window::jobs::PasterJobRuntime;
use crate::error::UiError;
use crate::ports::{ConfigStore, DesktopPort, MediaJobRunner};
use event_handler::{
    handle_workspace_about_to_wait, handle_workspace_user_event, handle_workspace_window_event,
};
use snapin_core::config::Config;
use std::collections::HashMap;
use std::sync::Arc;
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder};
#[cfg(target_os = "linux")]
use winit::platform::wayland::EventLoopBuilderExtWayland;
#[cfg(target_os = "windows")]
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::WindowId;

/// 贴图工作区启动参数。
///
/// 把配置、端口与生命周期策略打包传入，避免工作区在内部隐式加载配置，
/// 也让应用层可以针对守护进程与单次会话复用同一套事件循环实现。
pub struct PasterWorkspaceOptions {
    /// 工作区启动时的配置快照
    pub config: Config,
    /// 桌面副作用端口
    pub desktop: Arc<dyn DesktopPort>,
    /// 媒体任务执行端口
    pub media: Arc<dyn MediaJobRunner>,
    /// 配置持久化端口，用于保存活动分组等低频率设置
    pub config_store: Arc<dyn ConfigStore>,
    /// 单次会话模式下最后一个贴图关闭后是否退出事件循环
    pub exit_on_all_closed: bool,
}

/// 贴图工作区事件循环持有的全部长生命周期资源。
///
/// 集中封装窗口实例、分组管理器、全局配置与媒体任务运行时，
/// 既减少事件处理函数的平铺参数，也明确运行时的唯一归属方。
pub(crate) struct WorkspaceRuntime {
    pub instances: HashMap<WindowId, PasterInstance>,
    pub manager: PasterManager,
    pub cfg: Arc<Config>,
    pub jobs: Arc<PasterJobRuntime>,
    /// 桌面副作用端口
    pub desktop: Arc<dyn DesktopPort>,
    /// 配置持久化端口
    pub config_store: Arc<dyn ConfigStore>,
    /// 单次会话模式下最后一个贴图关闭后是否退出事件循环
    pub exit_on_all_closed: bool,
}

/// 创建集中多窗口贴图工作区事件循环与配套代理
pub fn create_paster_workspace(
) -> Result<(EventLoop<PasterAppEvent>, PasterWorkspaceProxy), UiError> {
    #[allow(unused_mut)]
    let mut builder = EventLoopBuilder::<PasterAppEvent>::with_user_event();
    #[cfg(target_os = "linux")]
    builder.with_any_thread(true);
    #[cfg(target_os = "windows")]
    builder.with_any_thread(true);
    let event_loop = builder.build().map_err(|e| UiError::Window {
        message: format!("创建集中多窗口贴图工作区事件循环失败: {e}"),
    })?;
    let proxy = PasterWorkspaceProxy::new(event_loop.create_proxy());
    Ok((event_loop, proxy))
}

/// 运行集中贴图工作区事件主循环
///
/// 当 `options.exit_on_all_closed` 为 false 时（常驻守护模式），即便当前贴图全部关闭，
/// 事件循环也始终常驻等待；为 true 时（单次会话模式），在最后一个贴图关闭后自动退出。
///
/// # Errors
///
/// 事件循环运行失败时返回 [`UiError::Window`]。后台媒体执行器由应用层持有并负责停机，
/// 工作区退出不会擅自销毁宿主执行器。
pub fn run_paster_workspace_loop(
    event_loop: EventLoop<PasterAppEvent>,
    options: PasterWorkspaceOptions,
) -> Result<(), UiError> {
    let PasterWorkspaceOptions {
        config,
        desktop,
        media,
        config_store,
        exit_on_all_closed,
    } = options;

    let mut manager = PasterManager::default();
    let cfg = Arc::new(config);
    let initial_group = cfg
        .paster
        .active_group_id
        .min(event_handler::TOTAL_PIN_GROUPS.saturating_sub(1));
    manager.switch_group(initial_group);

    let proxy = PasterWorkspaceProxy::new(event_loop.create_proxy());
    let jobs = Arc::new(PasterJobRuntime::new(media, proxy.clone()));
    let mut runtime = WorkspaceRuntime {
        instances: HashMap::new(),
        manager,
        cfg,
        jobs,
        desktop,
        config_store,
        exit_on_all_closed,
    };

    event_loop
        .run(move |event, elwt| {
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
        })
        .map_err(|e| UiError::Window {
            message: format!("运行集中贴图工作区事件主循环失败: {e}"),
        })
}
