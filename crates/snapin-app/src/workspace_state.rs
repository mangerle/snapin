//! 工作区运行时状态持久化。
//!
//! 活动分组描述当前会话位置，不是用户偏好，因此不写入 `config.toml`。
//! 独立状态文件避免每次切换分组重写完整配置，也降低配置热更新与分组切换的竞态。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use snapin_foundation::AppPaths;
use snapin_runtime::{TaskPool, TaskRuntime};
use snapin_ui::paster::TOTAL_PIN_GROUPS;
use snapin_ui::{PortError, PortErrorKind, WorkspaceState, WorkspaceStateStore};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc::{self, error::TrySendError};

/// 工作区状态文件允许读取的最大字节数。
const MAX_WORKSPACE_STATE_BYTES: usize = 64 * 1024;
/// 状态写入队列容量；分组切换是低频人工操作，满队列时快速失败而不是阻塞 GUI。
const WORKSPACE_STATE_QUEUE_CAPACITY: usize = 8;

/// 状态文件中的持久化结构，字段全部带默认值以兼容未来扩展。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedWorkspaceState {
    #[serde(default)]
    active_group_id: usize,
}

/// 默认运行时状态存储：`<cache>/workspace-state.toml`。
pub(crate) struct AppWorkspaceStateStore {
    path: PathBuf,
    save_tx: mpsc::Sender<WorkspaceState>,
}

impl AppWorkspaceStateStore {
    /// 使用默认缓存目录和统一后台运行时构建非阻塞状态存储。
    pub(crate) fn new(runtime: TaskRuntime) -> Result<Self, PortError> {
        Self::with_path(runtime, AppPaths::cache_dir().join("workspace-state.toml"))
    }

    fn with_path(runtime: TaskRuntime, path: PathBuf) -> Result<Self, PortError> {
        let (save_tx, mut save_rx) = mpsc::channel(WORKSPACE_STATE_QUEUE_CAPACITY);
        let writer_path = path.clone();
        runtime
            .spawn_in(TaskPool::Maintenance, async move {
                while let Some(state) = save_rx.recv().await {
                    let path = writer_path.clone();
                    let result =
                        tokio::task::spawn_blocking(move || save_file_at(&path, state)).await;
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            tracing::warn!("持久化贴图工作区状态失败: {error:#}");
                        }
                        Err(error) => {
                            tracing::warn!("等待贴图工作区状态写入任务失败: {error}");
                        }
                    }
                }
            })
            .map_err(|error| {
                PortError::with_kind(
                    PortErrorKind::Unavailable,
                    "启动工作区状态写入任务",
                    error.to_string(),
                )
            })?;
        Ok(Self { path, save_tx })
    }
}

impl WorkspaceStateStore for AppWorkspaceStateStore {
    fn load(&self) -> Result<WorkspaceState, PortError> {
        load_file_at(&self.path).map_err(|error| {
            PortError::with_kind(PortErrorKind::Io, "读取工作区状态", format!("{error:#}"))
        })
    }

    fn save(&self, state: WorkspaceState) -> Result<(), PortError> {
        self.save_tx.try_send(state).map_err(|error| match error {
            TrySendError::Full(_) => {
                PortError::with_kind(PortErrorKind::Busy, "保存工作区状态", "状态写入队列已满")
            }
            TrySendError::Closed(_) => PortError::with_kind(
                PortErrorKind::Unavailable,
                "保存工作区状态",
                "状态写入任务已停止",
            ),
        })
    }
}

fn load_file_at(path: &Path) -> Result<WorkspaceState> {
    let content = match snapin_foundation::read_utf8_file_limited(path, MAX_WORKSPACE_STATE_BYTES) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkspaceState::default());
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("读取工作区状态文件失败: {}", path.display()));
        }
    };
    let persisted: PersistedWorkspaceState = toml::from_str(&content)
        .with_context(|| format!("解析工作区状态文件失败: {}", path.display()))?;
    let active_group_id = persisted
        .active_group_id
        .min(TOTAL_PIN_GROUPS.saturating_sub(1));
    Ok(WorkspaceState { active_group_id })
}

fn save_file_at(path: &Path, state: WorkspaceState) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("创建工作区状态目录失败: {}", parent.display()))?;
    let persisted = PersistedWorkspaceState {
        active_group_id: state
            .active_group_id
            .min(TOTAL_PIN_GROUPS.saturating_sub(1)),
    };
    let content = toml::to_string(&persisted).context("序列化工作区状态失败")?;
    let temp_path = parent.join(format!(
        ".workspace-state.{}.{}.tmp",
        std::process::id(),
        AppPaths::generate_unique_id()
    ));
    AppPaths::write_secure_file(&temp_path, content.as_bytes())
        .with_context(|| format!("写入工作区状态临时文件失败: {}", temp_path.display()))?;
    if let Err(error) = AppPaths::replace_file_atomically(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error).with_context(|| format!("替换工作区状态文件失败: {}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_state_roundtrip_and_clamp() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("workspace-state.toml");
        save_file_at(&path, WorkspaceState { active_group_id: 2 }).expect("应可保存状态");
        assert_eq!(
            load_file_at(&path).expect("应可读取状态").active_group_id,
            2
        );

        save_file_at(
            &path,
            WorkspaceState {
                active_group_id: usize::MAX,
            },
        )
        .expect("应可保存状态");
        assert_eq!(
            load_file_at(&path).expect("应可读取状态").active_group_id,
            TOTAL_PIN_GROUPS - 1
        );
    }

    #[test]
    fn test_workspace_state_missing_file_returns_default() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("missing.toml");
        assert_eq!(
            load_file_at(&path)
                .expect("缺失状态应回退默认")
                .active_group_id,
            0
        );
    }

    #[test]
    fn test_workspace_state_rejects_oversized_file() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("workspace-state.toml");
        std::fs::write(
            &path,
            vec![b' '; MAX_WORKSPACE_STATE_BYTES.saturating_add(1)],
        )
        .expect("应可写入超限状态");
        assert!(load_file_at(&path).is_err(), "超限状态文件必须被拒绝");
    }

    #[tokio::test]
    async fn test_queued_workspace_state_is_flushed_before_shutdown() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let path = dir.path().join("workspace-state.toml");
        let runtime = TaskRuntime::current(2).expect("测试运行时必须可用");
        let store = AppWorkspaceStateStore::with_path(runtime.clone(), path.clone())
            .expect("状态存储应可启动");

        store
            .save(WorkspaceState { active_group_id: 2 })
            .expect("状态应成功入队");
        drop(store);
        assert!(runtime.shutdown(std::time::Duration::from_secs(1)).await);

        assert_eq!(
            load_file_at(&path).expect("状态应已落盘").active_group_id,
            2
        );
    }
}
