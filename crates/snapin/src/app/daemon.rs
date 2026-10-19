//! Snapin 常驻后台守护进程主服务循环模块。

use super::paste::run_oneshot_paste;
use anyhow::{Context, Result};
use snapin_core::config::Config;
use snapin_daemon::{
    send_command_to_daemon, DaemonCommand, DaemonResponse, IpcInvocation, IpcListener,
};
use snapin_platform::{send_desktop_notification, sync_hotkeys_to_gnome};
use std::path::PathBuf;
use tokio::sync::mpsc;

/// 运行后台守护服务
pub async fn run_daemon_server(cfg: Config) -> Result<()> {
    // 1. 若已有守护服务在运行，保持幂等性静默退出，不做多余改变
    if let Ok(true) = send_command_to_daemon(&DaemonCommand::Ping).await {
        tracing::info!("Snapin 守护服务已在后台运行中，无需重复启动。");
        let _ = std::process::Command::new("gnome-extensions")
            .args(["enable", "snapin@wangle.local"])
            .spawn();
        return Ok(());
    }

    tracing::info!("Snapin 守护服务启动中...");

    // 自动确保 GNOME 快捷键注册与配套扩展原地热重载
    let _ = sync_hotkeys_to_gnome(&cfg.hotkeys);
    let _ = std::process::Command::new("gnome-extensions")
        .args(["enable", "snapin@wangle.local"])
        .spawn();

    let listener = IpcListener::bind(None)
        .await
        .context("绑定 IPC 监听套接字失败")?;
    let (tx, mut rx) = mpsc::channel::<IpcInvocation>(32);

    tokio::spawn(async move {
        listener.run_loop(tx).await;
    });

    tracing::info!("Snapin 守护服务已就绪，等待热键指令...");
    if cfg.general.notify_on_startup {
        let _ = send_desktop_notification(
            "Snapin 截图贴图",
            "服务已就绪：按 Ctrl+Alt+A 截图，按 F1 贴图",
        )
        .await;
    }

    while let Some(inv) = rx.recv().await {
        tracing::info!("守护进程接收到控制指令: {:?}", inv.request);
        let resp = match inv.request {
            DaemonCommand::Snip => {
                let current_exe =
                    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("snapin"));
                if let Err(e) = std::process::Command::new(current_exe).arg("snip").spawn() {
                    tracing::error!("调起截图前台子进程失败: {e}");
                    DaemonResponse::Error(format!("调起截图子进程失败: {e}"))
                } else {
                    DaemonResponse::Ok
                }
            }
            DaemonCommand::Paste => {
                if let Err(e) = run_oneshot_paste().await {
                    tracing::error!("执行贴图流程失败: {e}");
                    DaemonResponse::Error(format!("执行贴图流程失败: {e}"))
                } else {
                    DaemonResponse::Ok
                }
            }
            DaemonCommand::TogglePasters => {
                tracing::info!("批量切换贴图显示状态");
                let _ = std::process::Command::new("pkill")
                    .args(["-USR1", "-f", "snapin pin-file"])
                    .output();
                DaemonResponse::Ok
            }
            DaemonCommand::SwitchGroup(id) => {
                tracing::info!("切换至贴图分组: {id}");
                let msg = format!("已切换至贴图分组: {id}");
                let _ = send_desktop_notification("Snapin 贴图组", &msg).await;
                DaemonResponse::Ok
            }
            DaemonCommand::Shutdown => {
                tracing::info!("接收到平滑停机指令，退出守护进程事件循环");
                let _ = inv.responder.send(DaemonResponse::Ok);
                break;
            }
            DaemonCommand::Ping => {
                tracing::debug!("心跳响应正常");
                DaemonResponse::Pong
            }
            DaemonCommand::GetStatus => DaemonResponse::Status {
                active_group: 1,
                paster_count: 0,
            },
        };
        let _ = inv.responder.send(resp);
    }

    Ok(())
}
