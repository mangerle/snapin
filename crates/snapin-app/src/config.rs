//! 应用配置共享快照与持久化适配。
//!
//! UI 与媒体执行器都需要读取同一份最新配置，但不应各自从磁盘加载。
//! 本模块用 [`ArcSwap`] 发布不可变配置快照：读路径无锁取得 `Arc`，写路径
//! 原子替换指针。配置属于典型读多写少数据，RCU 语义比全局读写锁更贴合访问模型。

use arc_swap::ArcSwap;
use snapin_config::config::Config;
use std::sync::Arc;

/// 进程内共享的配置快照。
pub(crate) type SharedConfig = Arc<ArcSwap<Config>>;

/// 创建共享配置快照。
pub(crate) fn shared_config(config: Config) -> SharedConfig {
    Arc::new(ArcSwap::from_pointee(config))
}

/// 读取当前配置快照。
pub(crate) fn config_snapshot(shared: &SharedConfig) -> Arc<Config> {
    shared.load_full()
}

/// 替换当前配置快照。
pub(crate) fn publish_config(shared: &SharedConfig, config: Config) {
    shared.store(Arc::new(config));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_snapshot_is_atomically_replaced_and_old_readers_keep_old_value() {
        let shared = shared_config(Config::default());
        let before = config_snapshot(&shared);

        let mut updated = Config::default();
        updated.general.update_check_interval_hours = 12;
        publish_config(&shared, updated);

        assert_eq!(
            config_snapshot(&shared).general.update_check_interval_hours,
            12
        );
        assert_eq!(
            before.general.update_check_interval_hours, 24,
            "替换不能改写已发出的配置快照"
        );
    }
}
