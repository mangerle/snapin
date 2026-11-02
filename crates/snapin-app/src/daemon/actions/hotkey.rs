//! 全局热键动作控制器。

use super::{spawn_clipboard_history_subprocess, spawn_snipper, spawn_workspace_command};
use crate::services::AppServices;
use crate::snip::SnipStartRequest;
use snapin_daemon::DaemonRequest;
use snapin_foundation::{PostCaptureAction, SnipMode};
use snapin_ui::paster::PasterWorkspaceProxy;
use std::sync::Arc;

/// 处理全局热键动作；返回 `false` 表示请求退出守护进程。
pub(crate) fn handle_hotkey_action(
    action: snapin_platform::HotkeyAction,
    services: &Arc<AppServices>,
    workspace: &PasterWorkspaceProxy,
) -> bool {
    match action {
        snapin_platform::HotkeyAction::Snip => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Normal,
                delay_seconds: 0,
                post_action: None,
            },
        ),
        snapin_platform::HotkeyAction::Translate => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Translate,
                delay_seconds: 0,
                post_action: None,
            },
        ),
        snapin_platform::HotkeyAction::Paste => {
            spawn_workspace_command(services, workspace, DaemonRequest::Paste);
        }
        snapin_platform::HotkeyAction::TogglePasters => {
            spawn_workspace_command(services, workspace, DaemonRequest::TogglePasters);
        }
        snapin_platform::HotkeyAction::SwitchGroup => {
            spawn_workspace_command(services, workspace, DaemonRequest::SwitchGroup(None));
        }
        snapin_platform::HotkeyAction::SmartRedact => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Normal,
                delay_seconds: 0,
                post_action: Some(PostCaptureAction::SmartRedact),
            },
        ),
        snapin_platform::HotkeyAction::Qr => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Normal,
                delay_seconds: 0,
                post_action: Some(PostCaptureAction::Qr),
            },
        ),
        snapin_platform::HotkeyAction::Beautify => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Normal,
                delay_seconds: 0,
                post_action: Some(PostCaptureAction::Beautify),
            },
        ),
        snapin_platform::HotkeyAction::Workspace => {
            spawn_workspace_command(services, workspace, DaemonRequest::OpenWorkspace);
        }
        snapin_platform::HotkeyAction::ClipboardHistory => {
            spawn_clipboard_history_subprocess();
        }
    }
    true
}
