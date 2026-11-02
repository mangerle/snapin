//! Snapin 常驻后台守护进程运行时。
//!
//! # 线程与事件循环模型
//!
//! 常驻进程采用“主线程 GUI + 后台异步服务”结构：
//! - winit 事件循环在进程主线程创建并运行，统一托管贴图窗口与截图覆盖层；
//! - Tokio 多线程运行时承载 IPC、托盘轮询、热键、周期更新与截图准备任务；
//! - 两个域通过有界通道和 `EventLoopProxy` 通信，GUI 线程从不执行网络或 OCR 阻塞操作。
//!
//! 这样既满足 winit 跨平台要求（事件循环必须在主线程且每进程仅一个），也消除了
//! 守护进程内启动第二个设置中心事件循环、以及每次截图派生新进程的问题。

mod actions;
pub mod commands;
mod shutdown;
mod update_loop;

use crate::services::AppServices;
use actions::{
    handle_hotkey_action, handle_ipc_request, handle_snipper_result, handle_tray_action,
};
use anyhow::{Context, Result};
use shutdown::ShutdownCoordinator;
use snapin_config::config::Config;
use snapin_daemon::{probe_daemon, DaemonProbe, IpcListener};
use snapin_platform::{
    enable_gnome_extension, send_desktop_notification, sync_hotkeys_to_gnome,
    PlatformHotkeyListener, PlatformTray,
};
use snapin_runtime::{TaskPool, TaskRuntime};
use snapin_ui::paster::{
    create_paster_workspace, run_paster_workspace_loop, PasterWorkspaceOptions,
    PasterWorkspaceProxy, SnipperFinishedCallback,
};
use snapin_ui::snipper::SnipperOutcome;
use std::sync::Arc;
use std::time::Duration;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc::{self, error::TrySendError};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use update_loop::run_update_check_loop;

/// 启动常驻守护进程：主线程运行 GUI 事件循环，后台运行时承载服务。
///
/// # Errors
///
/// 后台运行时创建、应用服务初始化、GUI 事件循环或服务任务失败时返回错误。
pub fn run_daemon_process(cfg: Config) -> Result<()> {
    snapin_foundation::i18n::set_language(cfg.general.language);
    let runtime = create_daemon_runtime()?;
    let handle = runtime.handle().clone();
    match probe_existing_daemon(&runtime) {
        ExistingDaemon::Online => {
            tracing::info!("Snapin 守护服务已在后台运行中，无需重复启动。");
            enable_gnome_extension();
            runtime.shutdown_timeout(Duration::from_secs(1));
            return Ok(());
        }
        ExistingDaemon::Unresponsive(reason) => {
            runtime.shutdown_timeout(Duration::from_secs(1));
            anyhow::bail!(
                "检测到正在运行但无法通信的 Snapin 守护进程（可能是旧版本或已无响应），\
                 请先结束该进程后重试，原因: {reason}"
            );
        }
        ExistingDaemon::Absent => {}
    }
    recover_update_on_startup();
    initialize_daemon_platform(&runtime, &cfg);

    let services = initialize_services(&cfg, &handle);
    let (event_loop, workspace_proxy) =
        create_paster_workspace().context("创建集中贴图工作区事件循环失败")?;
    let (snipper_callback, snipper_rx) = create_snipper_channel(&services);
    let (gui_closed_tx, gui_closed_rx) = watch::channel(false);
    let server = spawn_daemon_server(
        &handle,
        cfg.clone(),
        workspace_proxy.clone(),
        services.clone(),
        snipper_rx,
        gui_closed_rx,
    );
    let options = build_workspace_options(&cfg, &services, &workspace_proxy, snipper_callback)?;
    let gui_result = run_paster_workspace_loop(event_loop, options);
    let _ = gui_closed_tx.send(true);
    finish_daemon(runtime, server, gui_result)
}

/// 创建守护进程后台 Tokio 运行时。
fn create_daemon_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("snapin-daemon")
        .build()
        .context("创建守护进程后台运行时失败")
}

/// 启动探测结果。
enum ExistingDaemon {
    Online,
    Absent,
    Unresponsive(String),
}

/// 探测已有实例，不在借用运行时上执行销毁。
fn probe_existing_daemon(runtime: &tokio::runtime::Runtime) -> ExistingDaemon {
    match runtime.block_on(probe_daemon()) {
        DaemonProbe::Online => ExistingDaemon::Online,
        DaemonProbe::Unresponsive { reason } => ExistingDaemon::Unresponsive(reason),
        DaemonProbe::Absent => ExistingDaemon::Absent,
    }
}

/// 长驻入口启动时执行更新健康自检；失败只告警并继续。
fn recover_update_on_startup() {
    if let Err(error) = snapin_updater::check_and_recover_on_startup() {
        tracing::warn!("自更新启动自检失败，继续启动: {error}");
    }
}

/// 初始化平台能力、快捷键和 GNOME 扩展。
fn initialize_daemon_platform(runtime: &tokio::runtime::Runtime, cfg: &Config) {
    tracing::info!("Snapin 守护服务启动中...");
    runtime.block_on(snapin_platform::init_platform_capabilities());
    runtime.block_on(run_startup_hotkeys_and_extension(cfg));
    enable_gnome_extension();
    runtime.block_on(snapin_platform::refresh_platform_capabilities());
}

/// 构建统一任务运行时与应用服务，并启动剪贴板历史监控。
fn initialize_services(cfg: &Config, handle: &tokio::runtime::Handle) -> Arc<AppServices> {
    let task_runtime = TaskRuntime::from_handle(handle.clone(), 8);
    let services = Arc::new(AppServices::with_runtime(cfg.clone(), task_runtime));
    if let Err(error) = crate::run_clipboard_history_monitor(&services.runtime()) {
        tracing::warn!("启动剪贴板历史监控失败: {error:#}");
    }
    services
}

/// 创建有界截图结果通道与 GUI 线程回传回调。
fn create_snipper_channel(
    services: &Arc<AppServices>,
) -> (SnipperFinishedCallback, mpsc::Receiver<SnipperOutcome>) {
    let (snipper_tx, snipper_rx) = mpsc::channel::<SnipperOutcome>(4);
    let callback_runtime = services.runtime();
    let snipper_callback: SnipperFinishedCallback = Arc::new(move |outcome| {
        let sender = snipper_tx.clone();
        match sender.try_send(outcome) {
            Ok(()) => {}
            Err(TrySendError::Full(outcome)) => {
                // 回调运行在 GUI 线程，不能阻塞等待；把背压转交给统一运行时，
                // 由异步任务在队列出现空位后补投，避免直接丢弃贴图或保存结果。
                let sender = snipper_tx.clone();
                if let Err(error) = callback_runtime.spawn(async move {
                    let _ = sender.send(outcome).await;
                }) {
                    tracing::warn!("截图结果回传队列已满且异步补投失败: {error}");
                }
            }
            Err(TrySendError::Closed(_)) => {
                tracing::warn!("截图结果回传通道已关闭，工作区可能已退出");
            }
        }
    });
    (snipper_callback, snipper_rx)
}

/// 启动后台服务；服务异常时必须关闭 GUI，避免留下无服务空壳进程。
fn spawn_daemon_server(
    handle: &tokio::runtime::Handle,
    cfg: Config,
    workspace_proxy: PasterWorkspaceProxy,
    services: Arc<AppServices>,
    snipper_rx: mpsc::Receiver<SnipperOutcome>,
    gui_closed_rx: watch::Receiver<bool>,
) -> JoinHandle<Result<()>> {
    let shutdown_proxy = workspace_proxy.clone();
    let server_proxy = workspace_proxy.clone();
    handle.spawn(async move {
        let result =
            run_daemon_server(cfg, server_proxy, services, snipper_rx, gui_closed_rx).await;
        if result.is_err() {
            let _ = shutdown_proxy.shutdown_and_wait().await;
        }
        result
    })
}

/// 汇总主线程托盘、热键、桌面端口与截图回调，构建工作区选项。
fn build_workspace_options(
    cfg: &Config,
    services: &Arc<AppServices>,
    workspace_proxy: &PasterWorkspaceProxy,
    snipper_callback: SnipperFinishedCallback,
) -> Result<PasterWorkspaceOptions> {
    let tray = create_daemon_tray();
    let hotkey_listener = create_hotkey_listener(cfg);
    let on_tick = build_gui_tick(
        tray,
        hotkey_listener,
        services.clone(),
        workspace_proxy.clone(),
    );
    let state_store = crate::workspace_state::AppWorkspaceStateStore::new(services.runtime())
        .context("创建工作区状态存储失败")?;
    Ok(PasterWorkspaceOptions {
        config: cfg.clone(),
        desktop: services.desktop(),
        media: services.media(),
        state_store: Arc::new(state_store),
        on_snipper_finished: Some(snipper_callback),
        on_tick,
    })
}

/// 等待后台服务收尾并汇总 GUI/服务两侧的退出结果。
fn finish_daemon(
    runtime: tokio::runtime::Runtime,
    server: JoinHandle<Result<()>>,
    gui_result: Result<(), snapin_ui::UiError>,
) -> Result<()> {
    let server_result = runtime.block_on(async {
        match tokio::time::timeout(Duration::from_secs(10), server).await {
            Ok(result) => result.context("等待守护进程服务任务退出失败"),
            Err(_) => {
                tracing::warn!("等待守护进程服务任务退出超时（10 秒），继续收尾后台运行时");
                Ok(Ok(()))
            }
        }
    });
    runtime.shutdown_timeout(Duration::from_secs(5));

    match (gui_result, server_result) {
        (Err(error), _) => Err(error).context("守护进程 GUI 事件循环异常退出"),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(Ok(()))) => {
            tracing::info!("Snapin 守护服务已平滑退出");
            Ok(())
        }
        (Ok(()), Ok(Err(error))) => Err(error),
    }
}

/// 在主线程创建原生托盘；Windows/macOS 的原生托盘必须在主线程初始化。
fn create_daemon_tray() -> Option<PlatformTray> {
    let tray_icon = image::load_from_memory(include_bytes!("../../../../assets/icons/32x32.png"))
        .ok()
        .map(|img| {
            let rgba = img.to_rgba8();
            (rgba.as_raw().clone(), rgba.width(), rgba.height())
        });
    PlatformTray::new(tray_icon)
        .map_err(|error| tracing::warn!("初始化跨平台原生系统托盘失败: {error}"))
        .ok()
}

/// 在主线程注册原生全局热键监听器。
fn create_hotkey_listener(cfg: &Config) -> Option<PlatformHotkeyListener> {
    PlatformHotkeyListener::new(&cfg.hotkeys)
        .map_err(|error| tracing::warn!("初始化跨平台全局热键监听器失败: {error}"))
        .ok()
}

/// 构建 GUI 主线程周期回调：轮询托盘与热键，保证原生对象始终在创建它们的线程使用。
fn build_gui_tick(
    mut tray: Option<PlatformTray>,
    hotkey_listener: Option<PlatformHotkeyListener>,
    services: Arc<AppServices>,
    workspace: PasterWorkspaceProxy,
) -> Option<Box<dyn FnMut()>> {
    if tray.is_none() && hotkey_listener.is_none() {
        return None;
    }
    Some(Box::new(move || {
        if let Some(tray) = tray.as_mut() {
            if let Some(action) = tray.poll_action() {
                tracing::debug!("托盘触发操作: {action:?}");
                if !handle_tray_action(action, &services, &workspace) {
                    workspace.request_shutdown();
                }
            }
        }
        if let Some(hotkey_listener) = hotkey_listener.as_ref() {
            if let Some(action) = hotkey_listener.poll_action() {
                tracing::debug!("系统全局热键触发: {action:?}");
                if !handle_hotkey_action(action, &services, &workspace) {
                    workspace.request_shutdown();
                }
            }
        }
    }))
}

/// 启动阶段同步快捷键注册与扩展自愈。
async fn run_startup_hotkeys_and_extension(cfg: &Config) {
    let startup_hotkeys = cfg.hotkeys.clone();
    match tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "linux")]
        {
            if let Err(error) = crate::installer::install_gnome_extension() {
                tracing::debug!("自愈检查 GNOME 扩展部署状态: {error}");
            }
        }
        sync_hotkeys_to_gnome(&startup_hotkeys)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("启动守护服务时向系统注册快捷键失败: {error}"),
        Err(error) => tracing::warn!("快捷键注册任务异常: {error}"),
    }
}

/// 后台守护服务主循环。
async fn run_daemon_server(
    cfg: Config,
    workspace_proxy: PasterWorkspaceProxy,
    services: Arc<AppServices>,
    mut snipper_rx: mpsc::Receiver<SnipperOutcome>,
    mut gui_closed: watch::Receiver<bool>,
) -> Result<()> {
    let (mut rx, shutdown_tx, listener_handle) = start_ipc_listener().await?;
    #[cfg(unix)]
    let mut sigterm = signal(SignalKind::terminate()).context("注册 SIGTERM 信号监听失败")?;
    announce_daemon_startup(&cfg, &services).await;

    let mut snipper_channel_open = true;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("收到中断信号 (Ctrl+C)，开始平滑停机");
                break;
            }
            _ = async {
                #[cfg(unix)]
                {
                    sigterm.recv().await
                }
                #[cfg(not(unix))]
                {
                    std::future::pending::<Option<()>>().await
                }
            } => {
                tracing::info!("收到 SIGTERM，开始平滑停机");
                break;
            }
            changed = gui_closed.changed() => {
                if changed.is_err() || *gui_closed.borrow() {
                    tracing::warn!("GUI 工作区已退出，守护服务同步停止");
                    break;
                }
            }
            maybe_outcome = snipper_rx.recv(), if snipper_channel_open => {
                match maybe_outcome {
                    Some(outcome) => {
                        let services = Arc::clone(&services);
                        let workspace = workspace_proxy.clone();
                        if let Err(error) = services.runtime().spawn(async move {
                            handle_snipper_result(outcome, &services, &workspace).await;
                        }) {
                            tracing::warn!("提交截图结果处理任务失败: {error}");
                        }
                    }
                    None => snipper_channel_open = false,
                }
            }
            maybe_invocation = rx.recv() => {
                let Some(invocation) = maybe_invocation else {
                    tracing::info!("IPC 指令通道已关闭，准备退出守护进程");
                    break;
                };
                if handle_ipc_request(invocation, &services, &workspace_proxy) {
                    break;
                }
            }
        }
    }

    shutdown_daemon(
        workspace_proxy,
        gui_closed,
        shutdown_tx,
        listener_handle,
        &services,
    )
    .await
}

/// 启动 IPC 监听并返回指令接收端、停机信号与监听任务句柄。
async fn start_ipc_listener() -> Result<(
    mpsc::Receiver<snapin_daemon::IpcInvocation>,
    watch::Sender<bool>,
    JoinHandle<()>,
)> {
    let listener = IpcListener::bind(None)
        .await
        .context("绑定 IPC 监听套接字失败")?;
    let (tx, rx) = mpsc::channel(32);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let listener_handle = tokio::spawn(async move {
        listener.run_loop(tx, shutdown_rx).await;
    });
    Ok((rx, shutdown_tx, listener_handle))
}

/// 发送启动通知并按配置调度后台版本巡检。
async fn announce_daemon_startup(cfg: &Config, services: &Arc<AppServices>) {
    tracing::info!("Snapin 守护服务已就绪，等待热键指令...");
    let _ = snapin_updater::confirm_update_success();

    if cfg.general.notify_on_startup {
        if let Err(error) = send_desktop_notification(
            snapin_foundation::i18n::tr("Snapin 截图贴图"),
            snapin_foundation::i18n::tr("服务已就绪：按 Ctrl+Alt+A 截图，按 F1 贴图"),
        )
        .await
        {
            tracing::debug!("发送启动就绪通知失败或系统通知不可用: {error}");
        }
    }

    if cfg.general.auto_check_update {
        let check_url = cfg.general.custom_update_url.clone();
        let interval_hours = cfg.general.update_check_interval_hours.max(1);
        let runtime = services.runtime();
        let cancel = runtime.cancellation_token();
        if let Err(error) = runtime.spawn_in(TaskPool::Maintenance, async move {
            run_update_check_loop(check_url, interval_hours, cancel).await;
        }) {
            tracing::warn!("提交后台更新检查任务失败: {error}");
        }
    }
}

/// 平滑关闭 GUI、IPC 监听与统一后台任务运行时。
async fn shutdown_daemon(
    workspace_proxy: PasterWorkspaceProxy,
    gui_closed: watch::Receiver<bool>,
    shutdown_tx: watch::Sender<bool>,
    listener_handle: JoinHandle<()>,
    services: &Arc<AppServices>,
) -> Result<()> {
    ShutdownCoordinator::new()
        .shutdown(
            &workspace_proxy,
            &gui_closed,
            &shutdown_tx,
            listener_handle,
            services,
        )
        .await;
    Ok(())
}
