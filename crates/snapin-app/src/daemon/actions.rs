//! 守护进程托盘、全局热键与后台命令动作。

use super::commands::handle_command;
use crate::services::AppServices;
use crate::snip::{handle_snipper_outcome, start_snipper_task};
use snapin_core::SnipMode;
use snapin_daemon::DaemonRequest;
use snapin_platform::{spawn_detached_system_command, TrayAction};
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
pub(super) async fn handle_ipc_request(
    invocation: snapin_daemon::IpcInvocation,
    services: &Arc<AppServices>,
    workspace: &PasterWorkspaceProxy,
) -> bool {
    let is_shutdown = matches!(invocation.request, DaemonRequest::Shutdown);
    let response = handle_command(invocation.request, workspace, services).await;
    let _ = invocation.responder.send(response);
    if is_shutdown {
        tracing::info!("接收到平滑停机指令，退出守护进程事件循环");
    }
    is_shutdown
}

/// 处理托盘动作；返回 `false` 表示请求退出守护进程。
pub(super) fn handle_tray_action(
    action: TrayAction,
    services: &Arc<AppServices>,
    workspace: &PasterWorkspaceProxy,
) -> bool {
    match action {
        TrayAction::Snip => spawn_snipper(services, workspace, SnipMode::Normal, 0),
        TrayAction::Fullscreen => spawn_snipper(services, workspace, SnipMode::Fullscreen, 0),
        TrayAction::DelaySnip(seconds) => {
            spawn_snipper(services, workspace, SnipMode::Normal, seconds);
        }
        TrayAction::PickColor => spawn_snipper(services, workspace, SnipMode::PickColor, 0),
        TrayAction::AutoCopy => spawn_snipper(services, workspace, SnipMode::AutoCopy, 0),
        TrayAction::CustomSnip => spawn_snipper(services, workspace, SnipMode::Custom, 0),
        TrayAction::Paste => {
            let runtime = services.runtime();
            let services = Arc::clone(services);
            let workspace = workspace.clone();
            spawn_daemon_command(runtime, async move {
                let _ = handle_command(DaemonRequest::Paste, &workspace, &services).await;
            });
        }
        TrayAction::TogglePasters => {
            let runtime = services.runtime();
            let services = Arc::clone(services);
            let workspace = workspace.clone();
            spawn_daemon_command(runtime, async move {
                let _ = handle_command(DaemonRequest::TogglePasters, &workspace, &services).await;
            });
        }
        TrayAction::SwitchGroup => {
            let runtime = services.runtime();
            let services = Arc::clone(services);
            let workspace = workspace.clone();
            spawn_daemon_command(runtime, async move {
                let _ =
                    handle_command(DaemonRequest::SwitchGroup(None), &workspace, &services).await;
            });
        }
        TrayAction::ClearHistory => {
            let runtime = services.runtime();
            if let Err(error) = runtime.spawn(async {
                if let Err(error) = crate::cleaner::clear_snip_history().await {
                    tracing::warn!("托盘触发清空截屏历史失败: {error}");
                }
            }) {
                tracing::warn!("提交清空截屏历史任务失败: {error}");
            }
        }
        TrayAction::Preferences => spawn_preferences_subprocess(),
        TrayAction::Exit => {
            tracing::info!("托盘请求退出 Snapin，启动平滑停机流程");
            return false;
        }
    }
    true
}

/// 处理全局热键动作；返回 `false` 表示请求退出守护进程。
pub(super) fn handle_hotkey_action(
    action: snapin_platform::HotkeyAction,
    services: &Arc<AppServices>,
    workspace: &PasterWorkspaceProxy,
) -> bool {
    match action {
        snapin_platform::HotkeyAction::Snip => {
            spawn_snipper(services, workspace, SnipMode::Normal, 0)
        }
        snapin_platform::HotkeyAction::Translate => {
            spawn_snipper(services, workspace, SnipMode::Translate, 0)
        }
        snapin_platform::HotkeyAction::Paste => {
            let runtime = services.runtime();
            let services = Arc::clone(services);
            let workspace = workspace.clone();
            spawn_daemon_command(runtime, async move {
                let _ = handle_command(DaemonRequest::Paste, &workspace, &services).await;
            });
        }
        snapin_platform::HotkeyAction::TogglePasters => {
            let runtime = services.runtime();
            let services = Arc::clone(services);
            let workspace = workspace.clone();
            spawn_daemon_command(runtime, async move {
                let _ = handle_command(DaemonRequest::TogglePasters, &workspace, &services).await;
            });
        }
        snapin_platform::HotkeyAction::SwitchGroup => {
            let runtime = services.runtime();
            let services = Arc::clone(services);
            let workspace = workspace.clone();
            spawn_daemon_command(runtime, async move {
                let _ =
                    handle_command(DaemonRequest::SwitchGroup(None), &workspace, &services).await;
            });
        }
    }
    true
}

/// 提交一次截图覆盖层任务。
fn spawn_snipper(
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
fn spawn_daemon_command<F>(runtime: TaskRuntime, task: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if let Err(error) = runtime.spawn(task) {
        tracing::warn!("提交守护进程后台任务失败: {error}");
    }
}

/// 派生独立的设置中心进程，避免在守护进程内创建第二个 GUI 事件循环。
fn spawn_preferences_subprocess() {
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
