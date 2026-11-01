//! 托盘动作控制器。

use super::{spawn_preferences_subprocess, spawn_snipper, spawn_workspace_command};
use crate::services::AppServices;
use snapin_core::SnipMode;
use snapin_daemon::DaemonRequest;
use snapin_platform::TrayAction;
use snapin_ui::paster::PasterWorkspaceProxy;
use std::sync::Arc;

/// 处理托盘动作；返回 `false` 表示请求退出守护进程。
pub(crate) fn handle_tray_action(
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
            spawn_workspace_command(services, workspace, DaemonRequest::Paste);
        }
        TrayAction::TogglePasters => {
            spawn_workspace_command(services, workspace, DaemonRequest::TogglePasters);
        }
        TrayAction::SwitchGroup => {
            spawn_workspace_command(services, workspace, DaemonRequest::SwitchGroup(None));
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
