//! CLI 与常驻守护进程的启动握手。
//!
//! 所有 GUI 会话统一由守护进程主线程事件循环托管，因此截图、贴图与取色命令
//! 在发送 IPC 前必须先确保守护服务在线；若未运行，则派生当前可执行文件的
//! `daemon` 子命令并等待其 IPC 就绪。

use anyhow::{bail, Context, Result};
use snapin_daemon::{send_command_to_daemon, DaemonRequest};
use snapin_platform::spawn_detached_system_command;
use std::time::Duration;

/// 等待守护进程 IPC 就绪的最大时长。
const DAEMON_START_TIMEOUT: Duration = Duration::from_secs(5);
/// 轮询守护进程心跳的间隔。
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// 确保常驻守护进程在线；未运行时自动派生守护进程并等待 IPC 就绪。
///
/// # Errors
///
/// 无法定位当前可执行文件、请求始终无法送达或启动超时时返回错误。
pub async fn ensure_daemon_running() -> Result<()> {
    if ping_daemon().await {
        return Ok(());
    }

    let exe = std::env::current_exe().context("获取当前可执行文件路径失败")?;
    let exe = exe.to_str().context("当前可执行文件路径包含非法编码")?;
    tracing::info!("未检测到常驻守护服务，正在启动守护进程...");
    spawn_detached_system_command(exe, &["daemon"]);

    let attempts = (DAEMON_START_TIMEOUT.as_millis() / DAEMON_POLL_INTERVAL.as_millis()).max(1);
    for _ in 0..attempts {
        tokio::time::sleep(DAEMON_POLL_INTERVAL).await;
        if ping_daemon().await {
            tracing::info!("常驻守护服务已就绪");
            return Ok(());
        }
    }

    bail!(
        "等待常驻守护服务启动超时（{} 秒），请使用 `snapin daemon` 查看启动日志",
        DAEMON_START_TIMEOUT.as_secs()
    )
}

/// 发送守护进程心跳并判断是否在线。
async fn ping_daemon() -> bool {
    matches!(send_command_to_daemon(&DaemonRequest::Ping).await, Ok(true))
}
