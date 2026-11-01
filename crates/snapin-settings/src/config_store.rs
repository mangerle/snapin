//! 设置中心使用的已保存配置快照存储。
//!
//! # 设计原理
//!
//! 设置窗口同时存在两份数据：Slint 控件中的“待保存意图”和最近一次成功落盘的配置。
//! 旧实现把后者维护为 `Arc<Mutex<Config>>` 的影子副本，并允许“还原默认”直接改写它，
//! 导致未保存的默认值污染下一次保存，UI 未暴露的 `custom_update_url` 等字段被静默重置。
//!
//! 本模块把“已保存配置”收敛为只读快照：保存成功后才整体替换，读路径无锁竞争；
//! `ArcSwap` 的 RCU 语义也符合该配置读多写极少的访问模式。

use arc_swap::ArcSwap;
use snapin_core::config::Config;
use std::sync::Arc;

/// 最近一次成功保存的配置快照。
pub(crate) struct ConfigStore {
    current: ArcSwap<Config>,
}

impl ConfigStore {
    /// 以启动时加载的配置创建存储。
    pub(crate) fn new(config: Config) -> Self {
        Self {
            current: ArcSwap::from_pointee(config),
        }
    }

    /// 读取当前已保存配置的独立快照。
    pub(crate) fn snapshot(&self) -> Config {
        self.current.load().as_ref().clone()
    }

    /// 在保存成功后一次性替换快照，保证读者不会看到半更新状态。
    pub(crate) fn commit(&self, config: Config) {
        self.current.store(Arc::new(config));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 快照读取必须返回提交前的独立副本，提交后所有新读者看到新值。
    #[test]
    fn test_commit_replaces_snapshot_atomically() {
        let mut initial = Config::default();
        initial.general.update_check_interval_hours = 6;
        let store = ConfigStore::new(initial);

        let before = store.snapshot();
        assert_eq!(before.general.update_check_interval_hours, 6);

        let mut updated = before.clone();
        updated.general.update_check_interval_hours = 12;
        store.commit(updated);

        assert_eq!(store.snapshot().general.update_check_interval_hours, 12);
        assert_eq!(
            before.general.update_check_interval_hours, 6,
            "旧快照不能被后续提交改写"
        );
    }
}
