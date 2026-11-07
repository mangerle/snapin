//! 全局热键动作控制器。

use super::{spawn_clipboard_history_subprocess, spawn_snipper, spawn_workspace_command};
use crate::services::AppServices;
use crate::snip::SnipStartRequest;
use snapin_daemon::DaemonRequest;
use snapin_foundation::{PostCaptureAction, SnipMode};
use snapin_platform::HotkeyAction;
use snapin_ports::WorkspaceDriver;
use std::sync::Arc;

/// 处理全局热键动作；返回 `false` 表示请求退出守护进程。
pub(crate) fn handle_hotkey_action(
    action: HotkeyAction,
    services: &Arc<AppServices>,
    workspace: &Arc<dyn WorkspaceDriver>,
) -> bool {
    if let Some(request) = snip_request(action) {
        spawn_snipper(services, workspace, request);
    } else if let Some(request) = workspace_request(action) {
        spawn_workspace_command(services, workspace, request);
    } else if action == HotkeyAction::ClipboardHistory {
        spawn_clipboard_history_subprocess();
    }
    true
}

/// 将截图类热键转换为统一启动请求。
fn snip_request(action: HotkeyAction) -> Option<SnipStartRequest> {
    let (mode, post_action) = match action {
        HotkeyAction::Snip => (SnipMode::Normal, None),
        HotkeyAction::Translate => (SnipMode::Translate, None),
        HotkeyAction::SmartRedact => (SnipMode::Normal, Some(PostCaptureAction::SmartRedact)),
        HotkeyAction::Qr => (SnipMode::Normal, Some(PostCaptureAction::Qr)),
        HotkeyAction::Beautify => (SnipMode::Normal, Some(PostCaptureAction::Beautify)),
        _ => return None,
    };
    Some(SnipStartRequest {
        mode,
        delay_seconds: 0,
        post_action,
    })
}

/// 将贴图工作区类热键转换为守护命令。
fn workspace_request(action: HotkeyAction) -> Option<DaemonRequest> {
    match action {
        HotkeyAction::Paste => Some(DaemonRequest::Paste),
        HotkeyAction::TogglePasters => Some(DaemonRequest::TogglePasters),
        HotkeyAction::SwitchGroup => Some(DaemonRequest::SwitchGroup(None)),
        HotkeyAction::Workspace => Some(DaemonRequest::OpenWorkspace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 所有截图类热键必须映射到正确模式和一次性后处理动作。
    #[test]
    fn test_snipper_hotkey_routes_are_exhaustive() {
        let cases = [
            (HotkeyAction::Snip, SnipMode::Normal, None),
            (HotkeyAction::Translate, SnipMode::Translate, None),
            (
                HotkeyAction::SmartRedact,
                SnipMode::Normal,
                Some(PostCaptureAction::SmartRedact),
            ),
            (
                HotkeyAction::Qr,
                SnipMode::Normal,
                Some(PostCaptureAction::Qr),
            ),
            (
                HotkeyAction::Beautify,
                SnipMode::Normal,
                Some(PostCaptureAction::Beautify),
            ),
        ];
        for (action, mode, post_action) in cases {
            let request = snip_request(action).expect("截图类热键必须生成启动请求");
            assert_eq!(request.mode, mode);
            assert_eq!(request.delay_seconds, 0);
            assert_eq!(request.post_action, post_action);
        }
    }

    /// 工作区类热键必须映射到对应的守护命令。
    #[test]
    fn test_workspace_hotkey_routes_are_exhaustive() {
        assert_eq!(
            workspace_request(HotkeyAction::Paste),
            Some(DaemonRequest::Paste)
        );
        assert_eq!(
            workspace_request(HotkeyAction::TogglePasters),
            Some(DaemonRequest::TogglePasters)
        );
        assert_eq!(
            workspace_request(HotkeyAction::SwitchGroup),
            Some(DaemonRequest::SwitchGroup(None))
        );
        assert_eq!(
            workspace_request(HotkeyAction::Workspace),
            Some(DaemonRequest::OpenWorkspace)
        );
        assert_eq!(workspace_request(HotkeyAction::ClipboardHistory), None);
    }
}
