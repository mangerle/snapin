//! 守护进程托盘、全局热键与后台命令动作。

mod hotkey;
mod tray;

pub(super) use hotkey::handle_hotkey_action;
pub(super) use tray::handle_tray_action;

use super::commands::handle_command;
use crate::services::AppServices;
use crate::snip::{handle_snipper_outcome, start_snipper_task, SnipStartRequest};
use snapin_daemon::{DaemonErrorCode, DaemonRequest, DaemonResponse};
use snapin_platform::spawn_independent_process;
use snapin_ports::{SnipperOutcome, WorkspaceDriver};
use snapin_runtime::{CancellationToken, TaskRuntime};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// IPC 响应槽：确保后台任务无论正常结束、提前退出还是 panic，客户端都能收到回执。
///
/// 响应发送采用一次性取值语义，避免正常响应与析构兜底响应重复写入。任务在响应前
/// 发生 panic 时，最后一个槽引用析构会向等待方发送内部错误，而不是让连接静默超时。
struct ResponseSlot {
    sender: Mutex<Option<oneshot::Sender<DaemonResponse>>>,
}

impl ResponseSlot {
    /// 创建持有一次性响应发送端的槽位。
    fn new(sender: oneshot::Sender<DaemonResponse>) -> Self {
        Self {
            sender: Mutex::new(Some(sender)),
        }
    }

    /// 发送显式响应；槽位已被消费时忽略重复调用。
    fn send(&self, response: DaemonResponse) {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(sender) = sender {
            let _ = sender.send(response);
        }
    }
}

impl Drop for ResponseSlot {
    fn drop(&mut self) {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(sender) = sender {
            let _ = sender.send(DaemonResponse::error(
                DaemonErrorCode::Internal,
                "IPC 命令任务在返回结果前异常退出",
            ));
        }
    }
}

/// 处理一条常驻截图结果。
pub(super) async fn handle_snipper_result(
    outcome: SnipperOutcome,
    services: &Arc<AppServices>,
    workspace: &dyn WorkspaceDriver,
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
    workspace: &Arc<dyn WorkspaceDriver>,
) -> bool {
    if matches!(invocation.request, DaemonRequest::Shutdown) {
        let _ = invocation.responder.send(snapin_daemon::DaemonResponse::Ok);
        tracing::info!("接收到平滑停机指令，退出守护进程事件循环");
        return true;
    }

    let request = invocation.request;
    let responder = Arc::new(ResponseSlot::new(invocation.responder));
    let task_responder = Arc::clone(&responder);
    let cancelled = invocation.cancelled;
    let services = Arc::clone(services);
    let workspace = Arc::clone(workspace);
    if let Err(error) = services.runtime().spawn(async move {
        let response = handle_command(request, &workspace, &services, &cancelled).await;
        if let Some(response) = response {
            task_responder.send(response);
        }
    }) {
        tracing::warn!("提交 IPC 命令任务失败: {error}");
        responder.send(DaemonResponse::error(
            DaemonErrorCode::Busy,
            format!("提交 IPC 命令任务失败: {error}"),
        ));
    }
    false
}

/// 提交一次截图覆盖层任务。
pub(super) fn spawn_snipper(
    services: &Arc<AppServices>,
    workspace: &Arc<dyn WorkspaceDriver>,
    request: SnipStartRequest,
) {
    let services = Arc::clone(services);
    let workspace = Arc::clone(workspace);
    if let Err(error) = services.runtime().spawn(async move {
        start_snipper_task(services, workspace, request).await;
    }) {
        tracing::warn!("提交常驻截图任务失败: {error}");
    }
}

/// 提交一条无需回执的工作区命令。
///
/// 热键与托盘共享同一套“克隆服务容器 + 投递命令”样板，集中到此处避免两处
/// 动作分发逻辑随新增命令而漂移。
pub(super) fn spawn_workspace_command(
    services: &Arc<AppServices>,
    workspace: &Arc<dyn WorkspaceDriver>,
    request: DaemonRequest,
) {
    let runtime = services.runtime();
    let services = Arc::clone(services);
    let workspace = Arc::clone(workspace);
    spawn_daemon_command(runtime, async move {
        let cancel = CancellationToken::new();
        let _ = handle_command(request, &workspace, &services, &cancel).await;
    });
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
    spawn_independent_process(&exe, &["prefs"]);
}

/// 派生独立剪贴板历史面板进程；已有面板会收到关闭请求。
pub(super) fn spawn_clipboard_history_subprocess() {
    let Ok(exe) = std::env::current_exe() else {
        tracing::warn!("获取当前可执行文件路径失败，无法启动剪贴板历史面板");
        return;
    };
    spawn_independent_process(&exe, &["clipboard-history"]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常显式响应后，槽位析构不得再覆盖为内部错误。
    #[test]
    fn test_response_slot_sends_single_explicit_response() {
        let (sender, mut receiver) = oneshot::channel();
        let slot = ResponseSlot::new(sender);
        slot.send(DaemonResponse::Pong);
        drop(slot);

        assert_eq!(
            receiver.try_recv().expect("应收到显式响应"),
            DaemonResponse::Pong
        );
        assert!(receiver.try_recv().is_err(), "不得发送第二个响应");
    }

    /// 后台任务未响应即退出时，槽位析构必须提供可识别的内部错误回执。
    #[test]
    fn test_response_slot_drop_reports_internal_failure() {
        let (sender, mut receiver) = oneshot::channel();
        drop(ResponseSlot::new(sender));

        match receiver.try_recv().expect("应收到兜底响应") {
            DaemonResponse::Error(failure) => {
                assert_eq!(failure.code, DaemonErrorCode::Internal);
            }
            other => panic!("兜底响应类别错误，实际: {other:?}"),
        }
    }
}
