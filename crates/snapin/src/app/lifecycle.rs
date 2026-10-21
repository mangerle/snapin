//! Snapin 应用程序顶层生命周期调度（优雅退出与服务平滑停机）。

use anyhow::Result;
use snapin_daemon::{send_request_to_daemon, DaemonRequest};
use snapin_platform::lifecycle::stop_systemd_service;
use snapin_platform::set_extension_tray_visible;

/// 停止 Snapin 后台服务（保留配套 GNOME 扩展）
///
/// 1. 停止 systemd 用户服务以防被系统拉起；
/// 2. 请求扩展隐藏顶栏托盘，让用户看到"已退出"的直观结果；
/// 3. 异步向正在运行的守护进程发送平滑停机请求（由守护进程负责关闭各贴图窗口并清理套接字）；
/// 4. 刻意**不停用** GNOME 扩展，只隐藏托盘：扩展被停用后 GNOME 45+ 在本会话内无法重新加载，
///    会让贴图定位与零白闪截屏一起失效直到注销重登；隐藏托盘既符合"退出"的直观语义，
///    又保留了特权，守护进程下次启动时会把托盘重新显示出来；
/// 4. 彻底杜绝使用破坏性的全局字符串匹配 `pkill -9 -f`，避免误杀用户的编辑器、终端或同名目录进程。
pub async fn exit_snapin_application() -> Result<()> {
    tracing::info!("正在停止 Snapin 后台服务（GNOME 扩展保持启用）...");

    // 1. 停止 systemd 用户服务以防被系统重启守活
    stop_systemd_service();

    // 2. 隐藏顶栏托盘（扩展保持加载，特权不失效）
    if let Err(error) = set_extension_tray_visible(false).await {
        tracing::debug!("请求扩展隐藏托盘失败（扩展可能未启用）: {error}");
    }

    // 3. 异步通过 IPC 请求守护进程平滑停机
    if let Err(e) = send_request_to_daemon(&DaemonRequest::Shutdown).await {
        tracing::debug!("向守护进程发送停机请求未收到响应（服务可能未运行）: {e}");
    }

    tracing::info!("Snapin 已退出：后台服务已停止，顶栏托盘已隐藏");
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
