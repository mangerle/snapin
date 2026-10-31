//! Snapin 常驻后台守护进程主服务循环模块。

pub mod commands;

use anyhow::Context;
use commands::handle_command;
use snapin_core::config::Config;
use snapin_daemon::{send_command_to_daemon, DaemonRequest, IpcListener};
use snapin_platform::{
    enable_gnome_extension, send_desktop_notification, sync_hotkeys_to_gnome, PlatformTray,
    TrayAction,
};
use snapin_ui::paster::spawn_paster_workspace_thread;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

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
            if let Err(e) = super::installer::install_gnome_extension() {
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
    let workspace_proxy =
        spawn_paster_workspace_thread(false).context("初始化并启动集中贴图工作区失败")?;

    // 初始化跨平台原生系统托盘（在 Linux、Windows 与 macOS 平台提供系统托盘上下文菜单）
    let tray_icon =
        image::load_from_memory(include_bytes!("../../../../../assets/icons/32x32.png"))
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

    // 后台周期性轻量检查更新（模式 B：仅检查版本并发送桌面气泡通知，不下载大包）
    if cfg.general.auto_check_update {
        let check_url = cfg.general.custom_update_url.clone();
        let interval_hours = cfg.general.update_check_interval_hours.max(1);
        tokio::spawn(async move {
            run_update_check_loop(check_url, interval_hours).await;
        });
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

/// 守护进程后台更新轻量巡检循环（模式 B：仅提示不主动下载）
async fn run_update_check_loop(custom_url: Option<String>, interval_hours: u32) {
    // 延迟 10 秒发起首次检查，避开系统开机高峰期与网络重连抖动
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    let period = std::time::Duration::from_secs((interval_hours as u64).saturating_mul(3600));
    let mut check_interval = tokio::time::interval(period);

    loop {
        tracing::debug!("后台轻量检查 Snapin 新版本...");
        match snapin_core::check_update_async(custom_url.as_deref()).await {
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

        check_interval.tick().await;
    }
}
