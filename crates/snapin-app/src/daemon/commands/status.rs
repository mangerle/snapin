//! 贴图分组显隐、切换与状态查询指令。

use snapin_daemon::{DaemonErrorCode, DaemonResponse};
use snapin_platform::send_desktop_notification;
use snapin_ui::paster::GroupId;
use snapin_ui::paster::{is_valid_group_id, PasterWorkspaceProxy, TOTAL_PIN_GROUPS};
use std::time::Duration;
use tokio::time::timeout;

const STATUS_QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// 切换当前活动分组全部贴图的显隐。
pub(super) fn toggle(workspace: &PasterWorkspaceProxy) -> DaemonResponse {
    if let Err(error) = workspace.toggle_visibility() {
        DaemonResponse::error(
            DaemonErrorCode::Unavailable,
            format!("切换贴图显隐失败: {error}"),
        )
    } else {
        DaemonResponse::Ok
    }
}

/// 切换活动贴图分组并发送结果通知。
pub(super) async fn switch_group(
    workspace: &PasterWorkspaceProxy,
    target_id: Option<usize>,
) -> DaemonResponse {
    if let Some(id) = target_id {
        if !is_valid_group_id(GroupId::new(id)) {
            return DaemonResponse::error(
                DaemonErrorCode::InvalidInput,
                format!(
                    "非法贴图分组 ID: {id}，有效范围: 0..={}",
                    TOTAL_PIN_GROUPS.saturating_sub(1)
                ),
            );
        }
    }
    if let Err(error) = workspace.switch_group(target_id.map(GroupId::new)) {
        return DaemonResponse::error(
            DaemonErrorCode::Unavailable,
            format!("切换贴图分组失败: {error}"),
        );
    }
    // 与 SwitchGroup 走同一个事件队列，GetStatus 会在切换完成后执行，
    // 因此通知里可以带上真正生效的组号而不是客户端传入的目标值。
    let active_group = match timeout(STATUS_QUERY_TIMEOUT, workspace.get_status()).await {
        Ok(Ok(status)) => Some(status.active_group),
        Ok(Err(error)) => {
            tracing::warn!("查询切换后的贴图分组失败: {error}");
            None
        }
        Err(_) => {
            tracing::warn!("查询切换后的贴图分组超时");
            None
        }
    };
    let notification = match active_group {
        Some(group) => format!(
            "已切换至贴图组 {}/{}（ID {}）",
            group.get().saturating_add(1),
            TOTAL_PIN_GROUPS,
            group
        ),
        None => "已切换贴图活动分组".to_string(),
    };
    let _ = send_desktop_notification("Snapin 贴图组", &notification).await;
    DaemonResponse::Ok
}

/// 查询当前活动分组与贴图数量。
pub(super) async fn query(workspace: &PasterWorkspaceProxy) -> DaemonResponse {
    match timeout(STATUS_QUERY_TIMEOUT, workspace.get_status()).await {
        Ok(Ok(status)) => DaemonResponse::Status {
            active_group: status.active_group.get(),
            paster_count: status.paster_count,
        },
        Ok(Err(error)) => DaemonResponse::error(
            DaemonErrorCode::Unavailable,
            format!("查询贴图状态失败: {error}"),
        ),
        Err(_) => DaemonResponse::error(DaemonErrorCode::Timeout, "查询贴图状态超时"),
    }
}
