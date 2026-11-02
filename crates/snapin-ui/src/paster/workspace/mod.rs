//! 贴图子系统集中化单事件循环多窗口工作区调度模块。
//!
//! 负责由唯一的常驻 EventLoop 集中纳管多个独立的贴图窗口（WindowId），
//! 提供事件分发、分组显隐批量切换以及动态窗口生命周期管理，
//! 彻底消除每个贴图窗口独立创建进程与独立事件循环的反模式。
//!
//! 工作区只消费宿主注入的端口与配置快照，不自行读取配置文件、不创建后台线程；
//! GUI 线程的派生与后台执行器生命周期属于应用层职责。

pub mod event_handler;
pub mod job_events;
pub mod paster_events;
pub mod persistence;
pub mod proxy;
pub mod window_builder;

pub use event_handler::{is_valid_group_id, TOTAL_PIN_GROUPS};
pub use proxy::{PasterAppEvent, PasterWorkspaceProxy, WorkspaceStatus};
pub use window_builder::PasterInstance;

use super::manager::PasterManager;
use super::window::jobs::PasterJobRuntime;
use crate::error::UiError;
use crate::model::GroupId;
use crate::ports::{DesktopPort, MediaJobRunner, WorkspaceState, WorkspaceStateStore};
use crate::snipper::{SnipperOutcome, SnipperView};
use event_handler::{handle_workspace_user_event, handle_workspace_window_event};
use paster_events::handle_workspace_about_to_wait;
use snapin_config::config::Config;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
#[cfg(target_os = "linux")]
use winit::platform::wayland::EventLoopBuilderExtWayland;
#[cfg(target_os = "windows")]
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::WindowId;

/// 截图覆盖层结束回调。
///
/// 工作区运行在 GUI 事件循环线程，应用层不应在此执行耗时操作；回调应只做
/// 有界通道投递或轻量状态记录。
pub type SnipperFinishedCallback = Arc<dyn Fn(SnipperOutcome) + Send + Sync>;

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
    /// 运行时状态持久化端口，用于保存活动分组
    pub state_store: Arc<dyn WorkspaceStateStore>,
    /// 截图覆盖层结束回调；常驻守护进程用它把结果送回应用层
    pub on_snipper_finished: Option<SnipperFinishedCallback>,
    /// GUI 线程周期回调；用于在主线程轮询托盘与原生热键。
    ///
    /// 回调在主线程执行，不能捕获要求 `Send` 的宿主对象之外的限制；
    /// 返回后事件循环会安排 100ms 后的下一次唤醒。
    pub on_tick: Option<Box<dyn FnMut()>>,
}

/// 贴图工作区事件循环持有的全部长生命周期资源。
///
/// 集中封装窗口实例、分组管理器、全局配置与媒体任务运行时，
/// 既减少事件处理函数的平铺参数，也明确运行时的唯一归属方。
pub(crate) struct WorkspaceRuntime {
    pub instances: HashMap<WindowId, PasterInstance>,
    pub manager: PasterManager,
    pub cfg: Arc<Config>,
    pub jobs: Rc<PasterJobRuntime>,
    /// 桌面副作用端口
    pub desktop: Arc<dyn DesktopPort>,
    /// 运行时状态持久化端口
    pub state_store: Arc<dyn WorkspaceStateStore>,
    /// 当前截图覆盖层（同一时刻最多一个）
    pub snipper: Option<SnipperView>,
    /// 用于把截图任务结果投递回本事件循环的代理
    pub event_proxy: EventLoopProxy<PasterAppEvent>,
    /// 截图覆盖层结束回调；常驻守护进程用它把结果送回应用层
    pub on_snipper_finished: Option<SnipperFinishedCallback>,
    /// GUI 线程周期回调；用于在主线程轮询托盘与原生热键。
    ///
    /// 回调在主线程执行，不能捕获要求 `Send` 的宿主对象之外的限制；
    /// 返回后事件循环会安排 100ms 后的下一次唤醒。
    pub on_tick: Option<Box<dyn FnMut()>>,
}

/// 创建集中多窗口贴图工作区事件循环与配套代理
pub fn create_paster_workspace(
) -> Result<(EventLoop<PasterAppEvent>, PasterWorkspaceProxy), UiError> {
    let event_loop = build_workspace_event_loop()?;
    let proxy = PasterWorkspaceProxy::new(event_loop.create_proxy());
    Ok((event_loop, proxy))
}

/// 按平台创建事件循环；仅 Wayland/Windows 需要 `with_any_thread` 以支持后台线程创建。
fn build_workspace_event_loop() -> Result<EventLoop<PasterAppEvent>, UiError> {
    let build = |mut builder: winit::event_loop::EventLoopBuilder<PasterAppEvent>| {
        builder.build().map_err(|error| UiError::Window {
            message: format!("创建集中多窗口贴图工作区事件循环失败: {error}"),
        })
    };

    #[cfg(target_os = "linux")]
    {
        let mut builder = EventLoop::<PasterAppEvent>::with_user_event();
        builder.with_any_thread(true);
        build(builder)
    }

    #[cfg(target_os = "windows")]
    {
        let mut builder = EventLoop::<PasterAppEvent>::with_user_event();
        builder.with_any_thread(true);
        build(builder)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        build(EventLoop::<PasterAppEvent>::with_user_event())
    }
}

/// winit 0.30 应用回调适配器，集中托管工作区事件循环状态。
struct WorkspaceApplication {
    runtime: WorkspaceRuntime,
}

impl ApplicationHandler<PasterAppEvent> for WorkspaceApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: PasterAppEvent) {
        handle_workspace_user_event(event, &mut self.runtime, event_loop);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        handle_workspace_window_event(window_id, event, &mut self.runtime);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let deadline = handle_workspace_about_to_wait(&mut self.runtime);
        event_loop.set_control_flow(match deadline {
            Some(deadline) => ControlFlow::WaitUntil(deadline),
            None => ControlFlow::Wait,
        });
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        tracing::info!("集中多窗口贴图工作区事件循环退出");
    }
}

/// 运行集中贴图工作区事件主循环
///
/// 工作区是守护进程的唯一 GUI 事件循环，全部贴图关闭后仍常驻等待新指令。
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
        state_store,
        on_snipper_finished,
        on_tick,
    } = options;

    let mut manager = PasterManager::default();
    let cfg = Arc::new(config);
    let initial_group = state_store
        .load()
        .unwrap_or_else(|error| {
            tracing::warn!("读取工作区运行时状态失败，使用默认分组: {error}");
            WorkspaceState::default()
        })
        .active_group_id
        .min(event_handler::TOTAL_PIN_GROUPS.saturating_sub(1));
    manager.switch_group(GroupId::new(initial_group));

    let event_proxy = event_loop.create_proxy();
    let proxy = PasterWorkspaceProxy::new(event_proxy.clone());
    let jobs = Rc::new(PasterJobRuntime::new(media, proxy.clone()));
    let runtime = WorkspaceRuntime {
        instances: HashMap::new(),
        manager,
        cfg,
        jobs,
        desktop,
        state_store,
        snipper: None,
        event_proxy,
        on_snipper_finished,
        on_tick,
    };

    let mut application = WorkspaceApplication { runtime };
    event_loop
        .run_app(&mut application)
        .map_err(|e| UiError::Window {
            message: format!("运行集中贴图工作区事件主循环失败: {e}"),
        })
}
