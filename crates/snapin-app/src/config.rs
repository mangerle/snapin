//! 应用配置共享快照与持久化适配。
//!
//! UI 与媒体执行器都需要读取同一份最新配置，但不应各自从磁盘加载。
//! 本模块用 `RwLock<Arc<Config>>` 发布不可变配置快照：读路径只克隆 `Arc`，
//! 写路径整体替换，避免读写锁跨越耗时操作。

use snapin_core::config::{self, Config};
use snapin_ui::{ConfigStore, PortError};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// 进程内共享的配置快照。
pub(crate) type SharedConfig = Arc<RwLock<Arc<Config>>>;

/// 创建共享配置快照。
pub(crate) fn shared_config(config: Config) -> SharedConfig {
    Arc::new(RwLock::new(Arc::new(config)))
}

/// 读取当前配置快照；锁中毒时取回内部值继续，配置读失败不应拖垮 UI。
pub(crate) fn config_snapshot(shared: &SharedConfig) -> Arc<Config> {
    shared
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// 替换当前配置快照。
pub(crate) fn publish_config(shared: &SharedConfig, config: Config) {
    let mut guard = shared
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Arc::new(config);
}

/// 默认配置文件持久化适配。
///
/// `load` 与 `save` 都会同步更新共享快照，使 UI、媒体执行器与后续任务看到一致配置；
/// 磁盘格式与权限处理复用核心库既有实现，应用层只负责错误边界转换。
pub(crate) struct AppConfigStore {
    shared: SharedConfig,
}

impl AppConfigStore {
    /// 使用共享快照构建配置存储
    pub(crate) fn new(shared: SharedConfig) -> Self {
        Self { shared }
    }

    /// 获取配置文件路径。
    fn config_path(&self) -> Result<PathBuf, PortError> {
        config::get_default_config_path()
            .map_err(|error| PortError::new("定位配置文件", error.to_string()))
    }
}

impl ConfigStore for AppConfigStore {
    fn load(&self) -> Result<Config, PortError> {
        let loaded = config::load_or_create_config(None)
            .map_err(|error| PortError::new("加载配置", error.to_string()))?;
        publish_config(&self.shared, loaded.clone());
        Ok(loaded)
    }

    fn save(&self, config: &Config) -> Result<(), PortError> {
        let path = self.config_path()?;
        config::save_config(config, &path)
            .map_err(|error| PortError::new("保存配置", error.to_string()))?;
        publish_config(&self.shared, config.clone());
        Ok(())
    }
}
