//! Snapin 常驻后台守护进程主服务循环模块。

pub mod commands;

use crate::paster_workspace::spawn_paster_workspace_thread;
use crate::services::AppServices;
use anyhow::Context;
use commands::handle_command;
use snapin_core::config::Config;
use snapin_daemon::{send_command_to_daemon, DaemonRequest, IpcListener};
use snapin_platform::{
    enable_gnome_extension, send_desktop_notification, spawn_detached_system_command,
    sync_hotkeys_to_gnome, PlatformTray, TrayAction,
};
use std::sync::Arc;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

/// 派生独立的截图子进程执行交互截屏。
///
/// 架构设计原理：
/// Wayland 与 winit 的 EventLoop 在同一进程生命周期内只允许存在一个活跃实例。
/// 常驻守护进程已经持有集中贴图工作区的 GUI 事件循环，若直接在守护进程内部新创
/// 第二个 EventLoop 渲染全屏截图，会触发底层的事件循环锁死或崩溃。
/// 因此由守护进程派生极轻量的独立 `snapin snip` 子进程，既能干净分配窗口上下文，
/// 又保证截图结束后立即回收系统资源，并由系统进程守护安全收割。
fn spawn_snip_subprocess(args: &[&str]) {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_str) = exe.to_str() {
            spawn_detached_system_command(exe_str, args);
            return;
        }
    }
    spawn_detached_system_command("snapin", args);
}

/// 运行后台守护服务
pub async fn run_daemon_server(cfg: Config) -> anyhow::Result<()> {
    // 1. 若已有守护服务在运行，保持幂等性静默退出，不做多余改变
    if let Ok(true) = send_command_to_daemon(&DaemonRequest::Ping).await {
        tracing::info!("Snapin 守护服务已在后台运行中，无需重复启动。");
        enable_gnome_extension();
        return Ok(());
    }

    tracing::info!("Snapin 守护服务启动中...");

    // 自动确保 GNOME 快捷键注册与配套扩展原地自愈部署与热重载
    let startup_hotkeys = cfg.hotkeys.clone();
    match tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "linux")]
        {
            if let Err(e) = crate::installer::install_gnome_extension() {
                tracing::debug!("自愈检查 GNOME 扩展部署状态: {e}");
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
    enable_gnome_extension();
    let services = Arc::new(AppServices::new(cfg.clone()).context("初始化守护进程应用服务失败")?);
    let workspace_proxy = spawn_paster_workspace_thread(&services, false)
        .context("初始化并启动集中贴图工作区失败")?;

    // 初始化跨平台原生系统托盘（在 Linux、Windows 与 macOS 平台提供系统托盘上下文菜单）
    let tray_icon = image::load_from_memory(include_bytes!("../../../../assets/icons/32x32.png"))
        .ok()
        .map(|img| {
            let rgba = img.to_rgba8();
            (rgba.as_raw().clone(), rgba.width(), rgba.height())
        });

    let tray = PlatformTray::new(tray_icon)
        .map_err(|e| tracing::warn!("初始化跨平台原生系统托盘失败: {e}"))
        .ok();

    // 初始化跨平台全局快捷键监听器（在 Windows/macOS 平台捕获系统级按键）
    let hotkey_listener = snapin_platform::PlatformHotkeyListener::new(&cfg.hotkeys)
        .map_err(|e| tracing::warn!("初始化跨平台全局热键监听器失败: {e}"))
        .ok();

    // 3. 初始化并启动 IPC 监听服务，使用 watch 通道驱动平滑停机
    let listener = IpcListener::bind(None)
        .await
        .context("绑定 IPC 监听套接字失败")?;
    let (tx, mut rx) = mpsc::channel(32);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let listener_handle: JoinHandle<()> = tokio::spawn(async move {
        listener.run_loop(tx, shutdown_rx).await;
    });

    #[cfg(unix)]
    let mut sigterm = signal(SignalKind::terminate()).context("注册 SIGTERM 信号监听失败")?;

    tracing::info!("Snapin 守护服务已就绪，等待热键指令...");
    let _ = snapin_core::confirm_update_success();

    if cfg.general.notify_on_startup {
        if let Err(error) = send_desktop_notification(
            "Snapin 截图贴图",
            "服务已就绪：按 Ctrl+Alt+A 截图，按 F1 贴图",
        )
        .await
        {
            tracing::debug!("发送启动就绪通知失败或系统通知不可用: {error}");
        }
    }

    // 后台周期性轻量检查更新（模式 B：仅检查版本并发送桌面气泡通知，不下载大包）。
    // 任务提交到应用统一运行时并受取消令牌控制，守护进程停机时会等待其退出。
    if cfg.general.auto_check_update {
        let check_url = cfg.general.custom_update_url.clone();
        let interval_hours = cfg.general.update_check_interval_hours.max(1);
        let runtime = services.runtime();
        let cancel = runtime.cancellation_token();
        if let Err(error) = runtime.spawn(async move {
            run_update_check_loop(check_url, interval_hours, cancel).await;
        }) {
            tracing::warn!("提交后台更新检查任务失败: {error}");
        }
    }

    // 托盘菜单轮询定时器（每 100ms 探测一次非阻塞事件）
    let mut tray_interval = tokio::time::interval(std::time::Duration::from_millis(100));

    // 4. IPC 指令调度主循环：支持 SIGTERM/Ctrl-C 与内部 Shutdown 指令触发平滑停机
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
            _ = tray_interval.tick() => {
                if let Some(ref t) = tray {
                    if let Some(action) = t.poll_action() {
                        tracing::debug!("托盘触发操作: {:?}", action);
                        match action {
                            TrayAction::Snip => {
                                spawn_snip_subprocess(&["snip"]);
                            }
                            TrayAction::Fullscreen => {
                                spawn_snip_subprocess(&["snip", "--fullscreen"]);
                            }
                            TrayAction::DelaySnip(seconds) => {
                                let sec_str = seconds.to_string();
                                spawn_snip_subprocess(&["snip", "--delay", &sec_str]);
                            }
                            TrayAction::PickColor => {
                                spawn_snip_subprocess(&["pick-color"]);
                            }
                            TrayAction::AutoCopy => {
                                spawn_snip_subprocess(&["snip", "--auto-copy"]);
                            }
                            TrayAction::CustomSnip => {
                                spawn_snip_subprocess(&["snip", "--custom"]);
                            }
                            TrayAction::Paste => {
                                let _ = handle_command(DaemonRequest::Paste, &workspace_proxy, &services).await;
                            }
                            TrayAction::TogglePasters => {
                                let _ = handle_command(DaemonRequest::TogglePasters, &workspace_proxy, &services).await;
                            }
                            TrayAction::SwitchGroup => {
                                let _ = handle_command(DaemonRequest::SwitchGroup(None), &workspace_proxy, &services).await;
                            }
                            TrayAction::ClearHistory => {
                                let runtime = services.runtime();
                                if let Err(error) = runtime.spawn(async {
                                    if let Err(e) = crate::cleaner::clear_snip_history().await {
                                        tracing::warn!("托盘触发清空截屏历史失败: {e}");
                                    }
                                }) {
                                    tracing::warn!("提交清空截屏历史任务失败: {error}");
                                }
                            }
                            TrayAction::Preferences => {
                                tokio::task::spawn_blocking(|| {
                                    if let Err(e) = snapin_settings::run_settings_gui() {
                                        tracing::warn!("托盘启动偏好设置界面失败: {e}");
                                    }
                                });
                            }
                            TrayAction::Exit => {
                                tracing::info!("托盘请求退出 Snapin，启动平滑停机流程");
                                break;
                            }
                        }
                    }
                }

                if let Some(ref hk) = hotkey_listener {
                    if let Some(action) = hk.poll_action() {
                        tracing::debug!("系统全局热键触发: {:?}", action);
                        match action {
                            snapin_platform::HotkeyAction::Snip => {
                                spawn_snip_subprocess(&["snip"]);
                            }
                            snapin_platform::HotkeyAction::Translate => {
                                spawn_snip_subprocess(&["snip", "--translate"]);
                            }
                            snapin_platform::HotkeyAction::Paste => {
                                let _ = handle_command(DaemonRequest::Paste, &workspace_proxy, &services).await;
                            }
                            snapin_platform::HotkeyAction::TogglePasters => {
                                let _ = handle_command(DaemonRequest::TogglePasters, &workspace_proxy, &services).await;
                            }
                            snapin_platform::HotkeyAction::SwitchGroup => {
                                let _ = handle_command(DaemonRequest::SwitchGroup(None), &workspace_proxy, &services).await;
                            }
                        }
                    }
                }
            }
            maybe_invocation = rx.recv() => {
                let Some(invocation) = maybe_invocation else {
                    tracing::info!("IPC 指令通道已关闭，准备退出守护进程");
                    break;
                };
                let is_shutdown = matches!(invocation.request, DaemonRequest::Shutdown);
                let response = handle_command(invocation.request, &workspace_proxy, &services).await;
                let _ = invocation.responder.send(response);
                if is_shutdown {
                    tracing::info!("接收到平滑停机指令，退出守护进程事件循环");
                    break;
                }
            }
        }
    }

    // 5. 通知工作区关闭全部贴图窗口，停止 IPC 监听并等待其退出
    let _ = shutdown_tx.send(true);
    if let Err(error) = workspace_proxy.shutdown_and_wait().await {
        tracing::warn!("等待贴图工作区平滑退出失败: {error}");
    }
    match listener_handle.await {
        Ok(()) => {}
        Err(error) => tracing::warn!("IPC 监听任务异常退出: {error}"),
    }
    services.runtime().shutdown().await;

    tracing::info!("Snapin 守护服务已平滑退出");
    Ok(())
}

/// 守护进程后台更新轻量巡检循环（模式 B：仅提示不主动下载）。
///
/// 首次延迟 10 秒避开开机高峰；之后按配置间隔检查。每个等待点都监听统一取消
/// 令牌，守护进程停机时任务可以立即退出，不再遗留脱缰的 Tokio 任务。
async fn run_update_check_loop(
    custom_url: Option<String>,
    interval_hours: u32,
    cancel: snapin_runtime::CancellationToken,
) {
    let period = std::time::Duration::from_secs((interval_hours as u64).saturating_mul(3600));
    if wait_or_cancel(&cancel, std::time::Duration::from_secs(10)).await {
        return;
    }

    loop {
        tracing::debug!("后台轻量检查 Snapin 新版本...");
        let check = snapin_core::check_update_async(custom_url.as_deref());
        let check_result = tokio::select! {
            _ = cancel.cancelled() => return,
            result = check => result,
        };
        match check_result {
            Ok(Some(update)) => {
                let new_version = update.version().to_string();
                tracing::info!("检测到 Snapin 新版本: v{new_version}");
                let title = "Snapin 发现新版本";
                let body =
                    format!("检测到新版本 v{new_version}，可前往设置中心【关于】页查看并更新。");
                if let Err(error) = send_desktop_notification(title, &body).await {
                    tracing::debug!("发送版本更新桌面通知失败: {error}");
                }
            }
            Ok(None) => {
                tracing::debug!("当前已是最新版本，无需更新。");
            }
            Err(error) => {
                tracing::debug!("后台静默检查更新失败（已安全忽略）: {error}");
            }
        }

        if wait_or_cancel(&cancel, period).await {
            return;
        }
    }
}

/// 等待指定时长或取消信号；返回 `true` 表示已取消。
async fn wait_or_cancel(
    cancel: &snapin_runtime::CancellationToken,
    duration: std::time::Duration,
) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(duration) => false,
    }
}
