//! 托盘动作控制器。

use super::{
    spawn_clipboard_history_subprocess, spawn_preferences_subprocess, spawn_snipper,
    spawn_workspace_command,
};
use crate::services::AppServices;
use crate::snip::SnipStartRequest;
use snapin_daemon::DaemonRequest;
use snapin_foundation::{PostCaptureAction, SnipMode};
use snapin_platform::TrayAction;
use snapin_ports::WorkspaceDriver;
use std::sync::Arc;

/// 处理托盘动作；返回 `false` 表示请求退出守护进程。
pub(crate) fn handle_tray_action(
    action: TrayAction,
    services: &Arc<AppServices>,
    workspace: &Arc<dyn WorkspaceDriver>,
) -> bool {
    if let Some(request) = snip_request(action) {
        spawn_snipper(services, workspace, request);
        return true;
    }
    if let Some(request) = workspace_request(action) {
        spawn_workspace_command(services, workspace, request);
        return true;
    }

    match action {
        TrayAction::ClipboardHistory => spawn_clipboard_history_subprocess(),
        TrayAction::ClearHistory => clear_history(services),
        TrayAction::Preferences => spawn_preferences_subprocess(),
        TrayAction::Exit => {
            tracing::info!("托盘请求退出 Snapin，启动平滑停机流程");
            return false;
        }
        _ => {}
    }
    true
}

/// 将截图类托盘动作转换为统一启动请求。
fn snip_request(action: TrayAction) -> Option<SnipStartRequest> {
    let (mode, delay_seconds, post_action) = match action {
        TrayAction::Snip => (SnipMode::Normal, 0, None),
        TrayAction::Fullscreen => (SnipMode::Fullscreen, 0, None),
        TrayAction::DelaySnip(seconds) => (SnipMode::Normal, seconds, None),
        TrayAction::PickColor => (SnipMode::PickColor, 0, None),
        TrayAction::AutoCopy => (SnipMode::AutoCopy, 0, None),
        TrayAction::CustomSnip => (SnipMode::Custom, 0, None),
        TrayAction::SmartRedact => (SnipMode::Normal, 0, Some(PostCaptureAction::SmartRedact)),
        TrayAction::Qr => (SnipMode::Normal, 0, Some(PostCaptureAction::Qr)),
        TrayAction::Beautify => (SnipMode::Normal, 0, Some(PostCaptureAction::Beautify)),
        _ => return None,
    };
    Some(SnipStartRequest {
        mode,
        delay_seconds,
        post_action,
    })
}

/// 将工作区类托盘动作转换为守护命令。
fn workspace_request(action: TrayAction) -> Option<DaemonRequest> {
    match action {
        TrayAction::Paste => Some(DaemonRequest::Paste),
        TrayAction::Workspace => Some(DaemonRequest::OpenWorkspace),
        TrayAction::TogglePasters => Some(DaemonRequest::TogglePasters),
        TrayAction::SwitchGroup => Some(DaemonRequest::SwitchGroup(None)),
        _ => None,
    }
}

/// 在统一任务运行时中清理截图缓存。
fn clear_history(services: &Arc<AppServices>) {
    if let Err(error) = services.runtime().spawn(async {
        if let Err(error) = crate::cleaner::clear_snip_history().await {
            tracing::warn!("托盘触发清空截屏历史失败: {error}");
        }
    }) {
        tracing::warn!("提交清空截屏历史任务失败: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 所有截图类托盘项必须映射到正确的模式、延时和后处理动作。
    #[test]
    fn test_snipper_tray_routes_are_exhaustive() {
        let cases = [
            (TrayAction::Snip, SnipMode::Normal, 0, None),
            (TrayAction::Fullscreen, SnipMode::Fullscreen, 0, None),
            (TrayAction::DelaySnip(5), SnipMode::Normal, 5, None),
            (TrayAction::PickColor, SnipMode::PickColor, 0, None),
            (TrayAction::AutoCopy, SnipMode::AutoCopy, 0, None),
            (TrayAction::CustomSnip, SnipMode::Custom, 0, None),
            (
                TrayAction::SmartRedact,
                SnipMode::Normal,
                0,
                Some(PostCaptureAction::SmartRedact),
            ),
            (
                TrayAction::Qr,
                SnipMode::Normal,
                0,
                Some(PostCaptureAction::Qr),
            ),
            (
                TrayAction::Beautify,
                SnipMode::Normal,
                0,
                Some(PostCaptureAction::Beautify),
            ),
        ];
        for (action, mode, delay_seconds, post_action) in cases {
            let request = snip_request(action).expect("截图类托盘动作必须生成启动请求");
            assert_eq!(request.mode, mode);
            assert_eq!(request.delay_seconds, delay_seconds);
            assert_eq!(request.post_action, post_action);
        }
    }

    /// 工作区类托盘项必须映射到对应的守护命令。
    #[test]
    fn test_workspace_tray_routes_are_exhaustive() {
        assert_eq!(
            workspace_request(TrayAction::Paste),
            Some(DaemonRequest::Paste)
        );
        assert_eq!(
            workspace_request(TrayAction::Workspace),
            Some(DaemonRequest::OpenWorkspace)
        );
        assert_eq!(
            workspace_request(TrayAction::TogglePasters),
            Some(DaemonRequest::TogglePasters)
        );
        assert_eq!(
            workspace_request(TrayAction::SwitchGroup),
            Some(DaemonRequest::SwitchGroup(None))
        );
        assert_eq!(workspace_request(TrayAction::ClipboardHistory), None);
    }
}
