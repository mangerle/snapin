//! 守护进程 IPC 指令处理器。
//!
//! 指令按贴图、配置、状态三类拆分；所有文件与配置操作继续调度到阻塞线程池，
//! 避免阻塞异步主循环。禁用不可信路径由贴图指令在边界统一校验。

mod config;
mod paster;
mod status;

use crate::paste::paste_clipboard_to_workspace;
use crate::services::AppServices;
use paster::CreatePasterRequest;
use snapin_daemon::{DaemonRequest, DaemonResponse};
use snapin_ui::paster::PasterWorkspaceProxy;
use std::sync::Arc;

/// 处理单条守护进程指令。
pub async fn handle_command(
    command: DaemonRequest,
    workspace: &PasterWorkspaceProxy,
    services: &Arc<AppServices>,
) -> DaemonResponse {
    match command {
        DaemonRequest::ReloadConfig => config::reload_config(workspace, services.as_ref()).await,
        DaemonRequest::StartSnip {
            mode,
            delay_seconds,
            post_action,
        } => start_snip(
            services,
            workspace,
            crate::snip::SnipStartRequest {
                mode,
                delay_seconds,
                post_action,
            },
        ),
        DaemonRequest::Paste => paste_clipboard(workspace).await,
        DaemonRequest::CreatePaster {
            width,
            height,
            x,
            y,
            title,
            raw_path,
        } => {
            paster::create(
                workspace,
                CreatePasterRequest {
                    width,
                    height,
                    x,
                    y,
                    title,
                    raw_path,
                },
            )
            .await
        }
        DaemonRequest::TogglePasters => status::toggle(workspace),
        DaemonRequest::SwitchGroup(target_id) => status::switch_group(workspace, target_id).await,
        DaemonRequest::OpenWorkspace => {
            DaemonResponse::Error("贴图工作区面板尚未接入当前界面".to_string())
        }
        DaemonRequest::OpenClipboardHistory => {
            DaemonResponse::Error("剪贴板历史面板尚未接入当前界面".to_string())
        }
        DaemonRequest::ClearHistory => clear_history().await,
        DaemonRequest::Shutdown => DaemonResponse::Ok,
        DaemonRequest::Ping => DaemonResponse::Pong,
        DaemonRequest::GetStatus => status::query(workspace).await,
    }
}

/// 提交截图覆盖层任务；任务拥有独立的服务与工作区句柄。
fn start_snip(
    services: &Arc<AppServices>,
    workspace: &PasterWorkspaceProxy,
    request: crate::snip::SnipStartRequest,
) -> DaemonResponse {
    let services = Arc::clone(services);
    let workspace = workspace.clone();
    match services.runtime().spawn(async move {
        crate::snip::start_snipper_task(services, workspace, request).await;
    }) {
        Ok(_) => DaemonResponse::Ok,
        Err(error) => {
            tracing::warn!("提交截图覆盖层任务失败: {error}");
            DaemonResponse::Error(format!("提交截图任务失败: {error}"))
        }
    }
}

/// 在守护进程内执行清理，保证与媒体任务共享同一运行时并避免外部进程竞态。
async fn clear_history() -> DaemonResponse {
    match crate::cleaner::clear_snip_history().await {
        Ok(_) => DaemonResponse::Ok,
        Err(error) => {
            tracing::warn!("清理截屏历史失败: {error:#}");
            DaemonResponse::Error(format!("清理截屏历史失败: {error}"))
        }
    }
}

/// 从剪贴板生成贴图并投递到集中工作区。
async fn paste_clipboard(workspace: &PasterWorkspaceProxy) -> DaemonResponse {
    if let Err(error) = paste_clipboard_to_workspace(workspace).await {
        tracing::error!("守护服务执行剪贴板贴图失败: {error}");
        DaemonResponse::Error(format!("剪贴板贴图失败: {error}"))
    } else {
        DaemonResponse::Ok
    }
}
