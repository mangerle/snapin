//! 工作区运行时状态持久化。

use super::WorkspaceRuntime;
use crate::ports::WorkspaceState;

/// 将活动分组写入运行时状态文件，保证守护进程重启后恢复上次分组。
pub(super) fn persist_active_group(runtime: &mut WorkspaceRuntime, group: usize) {
    let state = WorkspaceState {
        active_group_id: group,
    };
    if let Err(error) = runtime.state_store.save(state) {
        tracing::warn!("持久化贴图活动分组失败: {error}");
    }
}
