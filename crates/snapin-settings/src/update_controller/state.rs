//! 自更新载荷与安装状态机。
//!
//! 状态与 UI 回调解耦，保证可用版本、已下载安装包和安装中状态只能出现一个，
//! 同时让控制器保持单一职责并便于独立验证状态迁移。

use crate::config_store::ConfigStore;
use snapin_runtime::{CancellationToken, TaskRuntime};
use snapin_updater::{DownloadedUpdate, UpdateInfo};
use std::sync::{Arc, Mutex, MutexGuard};

/// 自更新回调共享的运行时、取消令牌与状态容器。
#[derive(Clone)]
pub(super) struct UpdateContext {
    pub(super) runtime: TaskRuntime,
    pub(super) cancel: CancellationToken,
    pub(super) config: Arc<ConfigStore>,
    artifact: Arc<Mutex<UpdateArtifact>>,
}

/// 当前可用的更新载荷。
#[derive(Default)]
pub(super) enum UpdateArtifact {
    #[default]
    Empty,
    Available(UpdateInfo),
    Downloaded(DownloadedUpdate),
    Installing,
}

/// 安装按钮尝试读取载荷后的状态机结果。
pub(super) enum InstallStart {
    Started(Box<DownloadedUpdate>),
    AlreadyInstalling,
    Unavailable,
}

impl UpdateContext {
    pub(super) fn new(
        runtime: TaskRuntime,
        cancel: CancellationToken,
        config: Arc<ConfigStore>,
    ) -> Self {
        Self {
            runtime,
            cancel,
            config,
            artifact: Arc::new(Mutex::new(UpdateArtifact::Empty)),
        }
    }

    pub(super) fn replace_artifact(&self, artifact: UpdateArtifact) {
        *self.lock_artifact() = artifact;
    }

    pub(super) fn available_update(&self) -> Option<UpdateInfo> {
        match &*self.lock_artifact() {
            UpdateArtifact::Available(update) => Some(update.clone()),
            UpdateArtifact::Empty | UpdateArtifact::Downloaded(_) | UpdateArtifact::Installing => {
                None
            }
        }
    }

    pub(super) fn begin_install(&self) -> InstallStart {
        self.lock_artifact().begin_install()
    }

    pub(super) fn restore_downloaded(&self, downloaded: DownloadedUpdate) {
        self.replace_artifact(UpdateArtifact::Downloaded(downloaded));
    }

    fn lock_artifact(&self) -> MutexGuard<'_, UpdateArtifact> {
        self.artifact
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl UpdateArtifact {
    fn begin_install(&mut self) -> InstallStart {
        if matches!(*self, Self::Installing) {
            return InstallStart::AlreadyInstalling;
        }
        match std::mem::take(self) {
            Self::Downloaded(downloaded) => {
                *self = Self::Installing;
                InstallStart::Started(Box::new(downloaded))
            }
            _ => InstallStart::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空状态与安装中状态都必须拒绝再次取得安装包。
    #[test]
    fn test_install_state_machine_rejects_unavailable_and_duplicate_start() {
        let mut artifact = UpdateArtifact::Empty;
        assert!(matches!(
            artifact.begin_install(),
            InstallStart::Unavailable
        ));

        artifact = UpdateArtifact::Installing;
        assert!(matches!(
            artifact.begin_install(),
            InstallStart::AlreadyInstalling
        ));
    }
}
