//! 工作区运行时状态持久化。
//!
//! 活动分组描述当前会话位置，不是用户偏好，因此不写入 `config.toml`。
//! 独立状态文件避免每次切换分组重写完整配置，也降低配置热更新与分组切换的竞态。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use snapin_core::AppPaths;
use snapin_ui::paster::TOTAL_PIN_GROUPS;
use snapin_ui::{PortError, PortErrorKind, WorkspaceState, WorkspaceStateStore};
use std::path::{Path, PathBuf};

/// 状态文件中的持久化结构，字段全部带默认值以兼容未来扩展。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedWorkspaceState {
    #[serde(default)]
    active_group_id: usize,
}

/// 默认运行时状态存储：`<cache>/workspace-state.toml`。
pub(crate) struct AppWorkspaceStateStore {
    path: PathBuf,
}

impl AppWorkspaceStateStore {
    /// 使用默认缓存目录构建状态存储。
    pub(crate) fn new() -> Self {
        Self {
            path: AppPaths::cache_dir().join("workspace-state.toml"),
        }
    }

    /// 使用显式路径构建状态存储，供测试与未来可注入配置复用。
    #[cfg(test)]
    fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    fn load_file(&self) -> Result<WorkspaceState> {
        let content = match std::fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(WorkspaceState::default());
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("读取工作区状态文件失败: {}", self.path.display()));
            }
        };
        let persisted: PersistedWorkspaceState = toml::from_str(&content)
            .with_context(|| format!("解析工作区状态文件失败: {}", self.path.display()))?;
        let active_group_id = persisted
            .active_group_id
            .min(TOTAL_PIN_GROUPS.saturating_sub(1));
        Ok(WorkspaceState { active_group_id })
    }

    fn save_file(&self, state: WorkspaceState) -> Result<()> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
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
        if let Err(error) = std::fs::rename(&temp_path, &self.path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(error)
                .with_context(|| format!("替换工作区状态文件失败: {}", self.path.display()));
        }
        Ok(())
    }
}

impl WorkspaceStateStore for AppWorkspaceStateStore {
    fn load(&self) -> Result<WorkspaceState, PortError> {
        self.load_file().map_err(|error| {
            PortError::with_kind(PortErrorKind::Io, "读取工作区状态", format!("{error:#}"))
        })
    }

    fn save(&self, state: WorkspaceState) -> Result<(), PortError> {
        self.save_file(state).map_err(|error| {
            PortError::with_kind(PortErrorKind::Io, "保存工作区状态", format!("{error:#}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_state_roundtrip_and_clamp() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let store = AppWorkspaceStateStore::with_path(dir.path().join("workspace-state.toml"));
        store
            .save(WorkspaceState { active_group_id: 2 })
            .expect("应可保存状态");
        assert_eq!(store.load().expect("应可读取状态").active_group_id, 2);

        store
            .save(WorkspaceState {
                active_group_id: usize::MAX,
            })
            .expect("应可保存状态");
        assert_eq!(
            store.load().expect("应可读取状态").active_group_id,
            TOTAL_PIN_GROUPS - 1
        );
    }

    #[test]
    fn test_workspace_state_missing_file_returns_default() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let store = AppWorkspaceStateStore::with_path(dir.path().join("missing.toml"));
        assert_eq!(store.load().expect("缺失状态应回退默认").active_group_id, 0);
    }
}
