//! Snapin 应用程序顶层生命周期调度（优雅退出与服务平滑停机）。

use anyhow::Result;
use snapin_daemon::{send_request_to_daemon, DaemonRequest};
use snapin_platform::lifecycle::{disable_gnome_extension, stop_systemd_service};

/// 完全优雅退出 Snapin 应用程序
///
/// 1. 停止 systemd 用户服务以防被系统拉起；
/// 2. 禁用 GNOME 扩展以彻底收起顶栏托盘图标；
/// 3. 异步向正在运行的守护进程发送平滑停机请求（由守护进程负责关闭各贴图窗口并清理套接字）；
/// 4. 彻底杜绝使用破坏性的全局字符串匹配 `pkill -9 -f`，避免误杀用户的编辑器、终端或同名目录进程。
pub async fn exit_snapin_application() -> Result<()> {
    tracing::info!("正在完全退出 Snapin...");

    // 1. 停止 systemd 用户服务以防被系统重启守活
    stop_systemd_service();

    // 2. 禁用 GNOME 扩展以彻底收起顶栏托盘图标
    disable_gnome_extension();

    // 3. 异步通过 IPC 请求守护进程平滑停机
    if let Err(e) = send_request_to_daemon(&DaemonRequest::Shutdown).await {
        tracing::debug!("向守护进程发送停机请求未收到响应（服务可能未运行）: {e}");
    }

    tracing::info!("Snapin 应用程序已完成退出流程");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::exit_snapin_application;

    #[tokio::test]
    async fn test_exit_snapin_application_graceful_when_daemon_absent() {
        // 在守护服务未激活时调用退出函数应安全完成，不发生 panic
        assert!(exit_snapin_application().await.is_ok());
    }
}
