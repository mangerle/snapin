//! 应用配置共享快照与持久化适配。
//!
//! UI 与媒体执行器都需要读取同一份最新配置，但不应各自从磁盘加载。
//! 本模块用 `RwLock<Arc<Config>>` 发布不可变配置快照：读路径只克隆 `Arc`，
//! 写路径整体替换，避免读写锁跨越耗时操作。

use snapin_core::config::Config;
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
