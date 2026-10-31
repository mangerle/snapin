//! Snapin 常驻后台守护进程主服务循环模块。

pub mod commands;

use anyhow::Context;
use commands::handle_command;
use snapin_core::config::Config;
use snapin_daemon::{send_command_to_daemon, DaemonRequest, IpcListener};
use snapin_platform::{
    enable_gnome_extension, send_desktop_notification, set_extension_tray_visible,
    sync_hotkeys_to_gnome, PlatformTray, TrayAction,
};
use snapin_ui::paster::spawn_paster_workspace_thread;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

/// 把顶栏托盘恢复显示。
///
/// 退出 Snapin 时托盘被隐藏但扩展保持加载；守护进程再次启动（systemd 拉起、托盘菜单"重新启动"）
/// 时需要显式恢复。扩展可能刚被 enable 拉起、接口尚未就绪，这里在约 1 秒内重试若干次；
/// 仍失败只记录告警——下次登录时扩展加载会自动显示托盘。
async fn restore_extension_tray() {
    for attempt in 0..4u32 {
        match set_extension_tray_visible(true).await {
            Ok(()) => {
                tracing::debug!("已请求扩展恢复顶栏托盘（第 {} 次尝试）", attempt + 1);
                return;
            }
            Err(error) if attempt == 3 => {
                tracing::warn!("恢复顶栏托盘失败，扩展接口可能尚未就绪: {error}");
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
        }
    }
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

    // 自动确保 GNOME 快捷键注册与配套扩展原地热重载
    let startup_hotkeys = cfg.hotkeys.clone();
    match tokio::task::spawn_blocking(move || sync_hotkeys_to_gnome(&startup_hotkeys)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("启动守护服务时向系统注册快捷键失败: {error}"),
        Err(error) => tracing::warn!("快捷键注册任务异常: {error}"),
    }
    enable_gnome_extension();
    // 上次退出时只隐藏了托盘，这里把它恢复出来
    restore_extension_tray().await;
    let workspace_proxy =
        spawn_paster_workspace_thread(false).context("初始化并启动集中贴图工作区失败")?;

    // 初始化跨平台原生系统托盘（在 Windows/macOS 平台提供系统托盘上下文菜单）
    let tray = PlatformTray::new(None)
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
                                let snip_cfg = cfg.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = super::snip::run_oneshot_snip(
                                        snip_cfg,
                                        snapin_ui::snipper::SnipMode::Normal,
                                        0,
                                    )
                                    .await
                                    {
                                        tracing::warn!("托盘触发截图失败: {e}");
                                    }
                                });
                            }
                            TrayAction::Paste => {
                                let _ = handle_command(DaemonRequest::Paste, &workspace_proxy).await;
                            }
                            TrayAction::TogglePasters => {
                                let _ = handle_command(DaemonRequest::TogglePasters, &workspace_proxy).await;
                            }
                            TrayAction::Exit => {
                                tracing::info!("托盘请求退出 Snapin，启动平滑停机流程");
                                break;
                            }
                            _ => {}
                        }
                    }
                }

                if let Some(ref hk) = hotkey_listener {
                    if let Some(action) = hk.poll_action() {
                        tracing::debug!("系统全局热键触发: {:?}", action);
                        match action {
                            snapin_platform::HotkeyAction::Snip => {
                                let snip_cfg = cfg.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = super::snip::run_oneshot_snip(
                                        snip_cfg,
                                        snapin_ui::snipper::SnipMode::Normal,
                                        0,
                                    )
                                    .await
                                    {
                                        tracing::warn!("全局热键触发截图失败: {e}");
                                    }
                                });
                            }
                            snapin_platform::HotkeyAction::Translate => {
                                let snip_cfg = cfg.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = super::snip::run_oneshot_snip(
                                        snip_cfg,
                                        snapin_ui::snipper::SnipMode::Translate,
                                        0,
                                    )
                                    .await
                                    {
                                        tracing::warn!("全局热键触发截图翻译失败: {e}");
                                    }
                                });
                            }
                            snapin_platform::HotkeyAction::Paste => {
                                let _ = handle_command(DaemonRequest::Paste, &workspace_proxy).await;
                            }
                            snapin_platform::HotkeyAction::TogglePasters => {
                                let _ = handle_command(DaemonRequest::TogglePasters, &workspace_proxy).await;
                            }
                            snapin_platform::HotkeyAction::SwitchGroup => {
                                let _ = handle_command(DaemonRequest::SwitchGroup(None), &workspace_proxy).await;
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
                let response = handle_command(invocation.request, &workspace_proxy).await;
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

    tracing::info!("Snapin 守护服务已平滑退出");
    Ok(())
}
