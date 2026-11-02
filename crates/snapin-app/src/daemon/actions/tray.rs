//! 托盘动作控制器。

use super::{spawn_preferences_subprocess, spawn_snipper, spawn_workspace_command};
use crate::services::AppServices;
use crate::snip::SnipStartRequest;
use snapin_core::{PostCaptureAction, SnipMode};
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
        TrayAction::Snip => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Normal,
                delay_seconds: 0,
                post_action: None,
            },
        ),
        TrayAction::Fullscreen => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Fullscreen,
                delay_seconds: 0,
                post_action: None,
            },
        ),
        TrayAction::DelaySnip(seconds) => {
            spawn_snipper(
                services,
                workspace,
                SnipStartRequest {
                    mode: SnipMode::Normal,
                    delay_seconds: seconds,
                    post_action: None,
                },
            );
        }
        TrayAction::PickColor => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::PickColor,
                delay_seconds: 0,
                post_action: None,
            },
        ),
        TrayAction::AutoCopy => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::AutoCopy,
                delay_seconds: 0,
                post_action: None,
            },
        ),
        TrayAction::CustomSnip => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Custom,
                delay_seconds: 0,
                post_action: None,
            },
        ),
        TrayAction::Paste => {
            spawn_workspace_command(services, workspace, DaemonRequest::Paste);
        }
        TrayAction::SmartRedact => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Normal,
                delay_seconds: 0,
                post_action: Some(PostCaptureAction::SmartRedact),
            },
        ),
        TrayAction::Qr => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Normal,
                delay_seconds: 0,
                post_action: Some(PostCaptureAction::Qr),
            },
        ),
        TrayAction::Beautify => spawn_snipper(
            services,
            workspace,
            SnipStartRequest {
                mode: SnipMode::Normal,
                delay_seconds: 0,
                post_action: Some(PostCaptureAction::Beautify),
            },
        ),
        TrayAction::Workspace => {
            spawn_workspace_command(services, workspace, DaemonRequest::OpenWorkspace);
        }
        TrayAction::ClipboardHistory => {
            spawn_workspace_command(services, workspace, DaemonRequest::OpenClipboardHistory);
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
