//! 守护进程 IPC 指令处理器。
//!
//! 指令按贴图、配置、状态三类拆分；所有文件与配置操作继续调度到阻塞线程池，
//! 避免阻塞异步主循环。禁用不可信路径由贴图指令在边界统一校验。

mod config;
mod paster;
mod status;

use crate::paste::{paste_clipboard_to_workspace, PasteDispatchOutcome};
use crate::services::AppServices;
use paster::CreatePasterRequest;
use snapin_daemon::{DaemonErrorCode, DaemonRequest, DaemonResponse};
use snapin_runtime::CancellationToken;
use snapin_ui::workspace::PasterWorkspaceProxy;
use std::sync::Arc;

/// 处理单条守护进程指令。
///
/// 返回 `None` 表示命令在产生不可逆副作用前检测到取消。取消只在明确的阶段边界
/// 生效：安全的读取或计算可以完成后再丢弃结果，已经开始的事件投递、配置应用和
/// 系统集成则必须完整收尾，避免命令只执行一半。
pub async fn handle_command(
    command: DaemonRequest,
    workspace: &PasterWorkspaceProxy,
    services: &Arc<AppServices>,
    cancel: &CancellationToken,
) -> Option<DaemonResponse> {
    if cancel.is_cancelled() {
        return None;
    }

    match command {
        DaemonRequest::ReloadConfig => {
            Some(config::reload_config(workspace, services.as_ref()).await)
        }
        DaemonRequest::StartSnip {
            mode,
            delay_seconds,
            post_action,
        } => Some(start_snip(
            services,
            workspace,
            crate::snip::SnipStartRequest {
                mode,
                delay_seconds,
                post_action,
            },
        )),
        DaemonRequest::Paste => paste_clipboard(workspace, cancel).await,
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
                cancel,
            )
            .await
        }
        DaemonRequest::TogglePasters => Some(status::toggle(workspace)),
        DaemonRequest::SwitchGroup(target_id) => {
            Some(status::switch_group(workspace, target_id).await)
        }
        DaemonRequest::OpenWorkspace => Some(status::show(workspace)),
        DaemonRequest::ClearHistory => Some(clear_history().await),
        DaemonRequest::Shutdown => Some(DaemonResponse::Ok),
        DaemonRequest::Ping => Some(DaemonResponse::Pong),
        DaemonRequest::GetStatus => Some(status::query(workspace).await),
        DaemonRequest::CloseClipboardPanel => Some(DaemonResponse::error(
            DaemonErrorCode::Unsupported,
            "守护进程不处理剪贴板历史面板关闭请求",
        )),
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
            DaemonResponse::error(DaemonErrorCode::Busy, format!("提交截图任务失败: {error}"))
        }
    }
}

/// 在守护进程内执行清理，保证与媒体任务共享同一运行时并避免外部进程竞态。
async fn clear_history() -> DaemonResponse {
    match crate::cleaner::clear_snip_history().await {
        Ok(_) => DaemonResponse::Ok,
        Err(error) => {
            tracing::warn!("清理截屏历史失败: {error:#}");
            DaemonResponse::error(DaemonErrorCode::Io, format!("清理截屏历史失败: {error}"))
        }
    }
}

/// 从剪贴板生成贴图并投递到集中工作区。
async fn paste_clipboard(
    workspace: &PasterWorkspaceProxy,
    cancel: &CancellationToken,
) -> Option<DaemonResponse> {
    match paste_clipboard_to_workspace(workspace, cancel).await {
        Ok(PasteDispatchOutcome::Handled) => Some(DaemonResponse::Ok),
        Ok(PasteDispatchOutcome::Cancelled) => None,
        Err(error) => {
            tracing::warn!("守护服务执行剪贴板贴图失败: {error}");
            Some(DaemonResponse::error(
                DaemonErrorCode::Unavailable,
                format!("剪贴板贴图失败: {error}"),
            ))
        }
    }
}
