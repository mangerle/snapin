//! 全局热键动作控制器。

use super::{spawn_snipper, spawn_workspace_command};
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
            spawn_workspace_command(services, workspace, DaemonRequest::Paste);
        }
        snapin_platform::HotkeyAction::TogglePasters => {
            spawn_workspace_command(services, workspace, DaemonRequest::TogglePasters);
        }
        snapin_platform::HotkeyAction::SwitchGroup => {
            spawn_workspace_command(services, workspace, DaemonRequest::SwitchGroup(None));
        }
    }
    true
}
