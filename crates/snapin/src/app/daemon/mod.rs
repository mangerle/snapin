//! Snapin 常驻后台守护进程主服务循环模块。

pub mod commands;

use anyhow::Context;
use commands::handle_command;
use snapin_core::config::Config;
use snapin_daemon::{send_command_to_daemon, DaemonRequest, IpcListener};
use snapin_platform::{enable_gnome_extension, send_desktop_notification, sync_hotkeys_to_gnome};
use snapin_ui::paster::spawn_paster_workspace_thread;
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

    // 自动确保 GNOME 快捷键注册与配套扩展原地热重载
    let startup_hotkeys = cfg.hotkeys.clone();
    match tokio::task::spawn_blocking(move || sync_hotkeys_to_gnome(&startup_hotkeys)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("启动守护服务时向系统注册快捷键失败: {error}"),
        Err(error) => tracing::warn!("快捷键注册任务异常: {error}"),
    }
    enable_gnome_extension();

    // 2. 启动贴图集中工作区（常驻单一 GUI 事件循环独立线程）
    let workspace_proxy =
        spawn_paster_workspace_thread(false).context("初始化并启动集中贴图工作区失败")?;

    // 3. 初始化并启动 IPC 监听服务，使用 watch 通道驱动平滑停机
    let listener = IpcListener::bind(None)
        .await
        .context("绑定 IPC 监听套接字失败")?;
    let (tx, mut rx) = mpsc::channel(32);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let listener_handle: JoinHandle<()> = tokio::spawn(async move {
        listener.run_loop(tx, shutdown_rx).await;
    });

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

    // 4. IPC 指令调度主循环：支持 SIGTERM 与内部 Shutdown 指令触发平滑停机
    loop {
        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("收到 SIGTERM，开始平滑停机");
                break;
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
