//! 全局热键动作控制器。

use super::{spawn_daemon_command, spawn_snipper};
use crate::daemon::commands::handle_command;
use crate::services::AppServices;
use snapin_core::SnipMode;
use snapin_daemon::DaemonRequest;
use snapin_ui::paster::PasterWorkspaceProxy;
use std::sync::Arc;

/// 处理全局热键动作；返回 `false` 表示请求退出守护进程。
pub(crate) fn handle_hotkey_action(
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
