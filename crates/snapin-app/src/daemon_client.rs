//! CLI 与常驻守护进程的启动握手。
//!
//! 所有 GUI 会话统一由守护进程主线程事件循环托管，因此截图、贴图与取色命令
//! 在发送 IPC 前必须先确保守护服务在线；若未运行，则派生当前可执行文件的
//! `daemon` 子命令并等待其 IPC 就绪。

use anyhow::{bail, Context, Result};
use snapin_daemon::{probe_daemon, DaemonProbe};
use snapin_platform::spawn_detached_system_command;
use std::time::Duration;

/// 等待守护进程 IPC 就绪的最大时长。
const DAEMON_START_TIMEOUT: Duration = Duration::from_secs(5);
/// 轮询守护进程心跳的间隔。
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// 确保常驻守护进程在线；未运行时自动派生守护进程并等待 IPC 就绪。
///
/// 设计原理：探测结果必须区分“没有实例”与“实例存在但协议不兼容”。
/// 升级后旧版本守护进程仍可能占用套接字，若把它当作未运行而派生新实例，
/// 两个进程会各自注册托盘图标与热键，表现为“每次截图多出一个托盘”。
/// 因此仅在确实无人监听时派生，其余情况直接给出可定位的中文错误。
///
/// # Errors
///
/// 检测到无法通信的既有实例、无法定位当前可执行文件、请求始终无法送达
/// 或启动超时时返回错误。
pub async fn ensure_daemon_running() -> Result<()> {
    match probe_daemon().await {
        DaemonProbe::Online => return Ok(()),
        DaemonProbe::Unresponsive { reason } => bail!(
            "检测到正在运行但无法通信的 Snapin 守护进程（可能是旧版本或已无响应），\
             请先结束该进程后重试，原因: {reason}"
        ),
        DaemonProbe::Absent => {}
    }

    let exe = std::env::current_exe().context("获取当前可执行文件路径失败")?;
    let exe = exe.to_str().context("当前可执行文件路径包含非法编码")?;
    tracing::info!("未检测到常驻守护服务，正在启动守护进程...");
    spawn_detached_system_command(exe, &["daemon"]);

    let attempts = (DAEMON_START_TIMEOUT.as_millis() / DAEMON_POLL_INTERVAL.as_millis()).max(1);
    for _ in 0..attempts {
        tokio::time::sleep(DAEMON_POLL_INTERVAL).await;
        if matches!(probe_daemon().await, DaemonProbe::Online) {
            tracing::info!("常驻守护服务已就绪");
            return Ok(());
        }
    }

    // 超时后再探测一次：若对端存在但协议不兼容，给出比“启动超时”更可操作的提示。
    if let DaemonProbe::Unresponsive { reason } = probe_daemon().await {
        bail!(
            "启动守护进程失败：套接字已被无法通信的实例占用（可能为旧版本），\
             请先结束该进程后重试，原因: {reason}"
        );
    }

    bail!(
        "等待常驻守护服务启动超时（{} 秒），请使用 `snapin daemon` 查看启动日志",
        DAEMON_START_TIMEOUT.as_secs()
    )
}
