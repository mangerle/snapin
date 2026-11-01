//! 守护进程托盘、全局热键与后台命令动作。

mod hotkey;
mod tray;

pub(super) use hotkey::handle_hotkey_action;
pub(super) use tray::handle_tray_action;

use super::commands::handle_command;
use crate::services::AppServices;
use crate::snip::{handle_snipper_outcome, start_snipper_task};
use snapin_core::SnipMode;
use snapin_daemon::DaemonRequest;
use snapin_platform::spawn_detached_system_command;
use snapin_runtime::TaskRuntime;
use snapin_ui::paster::PasterWorkspaceProxy;
use snapin_ui::snipper::SnipperOutcome;
use std::sync::Arc;

/// 处理一条常驻截图结果。
pub(super) async fn handle_snipper_result(
    outcome: SnipperOutcome,
    services: &Arc<AppServices>,
    workspace: &PasterWorkspaceProxy,
) {
    let config = (*services.config()).clone();
    if let Err(error) = handle_snipper_outcome(&config, outcome, workspace).await {
        tracing::warn!("处理常驻截图结果失败: {error:#}");
    }
}

/// 处理一条 IPC 请求；返回 `true` 表示请求停机。
///
/// 主事件循环只负责路由：除停机指令需要立即改变循环状态外，其余业务命令全部
/// 提交到统一任务运行时执行。这样慢命令（创建贴图、切换分组、配置重载）不会
/// 阻塞 SIGTERM、GUI 退出与截图结果处理，也不会造成 IPC 队头阻塞。
pub(super) fn handle_ipc_request(
    invocation: snapin_daemon::IpcInvocation,
    services: &Arc<AppServices>,
    workspace: &PasterWorkspaceProxy,
) -> bool {
    if matches!(invocation.request, DaemonRequest::Shutdown) {
        let _ = invocation.responder.send(snapin_daemon::DaemonResponse::Ok);
        tracing::info!("接收到平滑停机指令，退出守护进程事件循环");
        return true;
    }

    let request = invocation.request;
    let responder = invocation.responder;
    let cancelled = invocation.cancelled;
    let services = Arc::clone(services);
    let workspace = workspace.clone();
    if let Err(error) = services.runtime().spawn(async move {
        let response = tokio::select! {
            _ = cancelled.cancelled() => return,
            response = handle_command(request, &workspace, &services) => response,
        };
        // 客户端可能已超时离开；发送失败只表示无人接收，业务结果已按需落盘。
        let _ = responder.send(response);
    }) {
        tracing::warn!("提交 IPC 命令任务失败: {error}");
    }
    false
}

/// 提交一次截图覆盖层任务。
pub(super) fn spawn_snipper(
    services: &Arc<AppServices>,
    workspace: &PasterWorkspaceProxy,
    mode: SnipMode,
    delay_seconds: u32,
) {
    let services = Arc::clone(services);
    let workspace = workspace.clone();
    if let Err(error) = services.runtime().spawn(async move {
        start_snipper_task(services, workspace, mode, delay_seconds).await;
    }) {
        tracing::warn!("提交常驻截图任务失败: {error}");
    }
}

/// 在统一运行时中提交一条守护命令任务。
pub(super) fn spawn_daemon_command<F>(runtime: TaskRuntime, task: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Err(error) = runtime.spawn(task) {
        tracing::warn!("提交守护进程后台任务失败: {error}");
    }
}

/// 派生独立的设置中心进程，避免在守护进程内创建第二个 GUI 事件循环。
pub(super) fn spawn_preferences_subprocess() {
    let Ok(exe) = std::env::current_exe() else {
        tracing::warn!("获取当前可执行文件路径失败，无法启动设置中心");
        return;
    };
    let Some(exe) = exe.to_str() else {
        tracing::warn!("当前可执行文件路径包含非法编码，无法启动设置中心");
        return;
    };
    spawn_detached_system_command(exe, &["prefs"]);
}
