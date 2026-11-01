//! 贴图多窗口生命周期调度与多分组隔离管理器。
//!
//! 管理器只维护贴图 ID、分组与可见性等轻量元数据，不持有像素缓冲区，
//! 像素数据由各贴图窗口独占，避免每张图在窗口与管理器之间发生多次深拷贝。
//!
//! 分组可见性由 `group_visible` 保存权威状态，`PasterEntry::is_visible` 只是
//! 该状态与活动分组的派生结果；窗口的 `set_visible` 同样是表现层动作，不再在
//! 窗口状态中保存第二份可见性布尔值。这样批量隐藏后新建贴图不会“偷偷显示”。

use crate::model::{GroupId, PasterId};
use std::collections::HashMap;

/// 单张贴图的元数据条目
#[derive(Debug, Clone, PartialEq, Eq)]
struct PasterEntry {
    id: PasterId,
    group_id: GroupId,
    is_visible: bool,
}

/// 贴图分组管理器
#[derive(Default)]
pub struct PasterManager {
    active_group: GroupId,
    entries: Vec<PasterEntry>,
    /// 每个分组的权威显隐状态；缺省视为可见。
    group_visible: HashMap<GroupId, bool>,
}

impl PasterManager {
    /// 获取当前活动分组 ID
    pub fn active_group(&self) -> GroupId {
        self.active_group
    }

    /// 登记一张新贴图；ID 重复时返回 `false`，可见性由活动分组与分组隐藏态共同决定。
    pub fn add_entry(&mut self, id: PasterId, group_id: GroupId) -> bool {
        if self.entries.iter().any(|entry| entry.id == id) {
            return false;
        }

        self.entries.push(PasterEntry {
            id,
            group_id,
            is_visible: group_id == self.active_group && self.group_is_visible(self.active_group),
        });
        true
    }

    /// 切换当前活动分组，并同步组内贴图可见性。
    pub fn switch_group(&mut self, group_id: GroupId) {
        self.active_group = group_id;
        let visible = self.group_is_visible(group_id);
        for entry in &mut self.entries {
            entry.is_visible = entry.group_id == group_id && visible;
        }
    }

    /// 批量切换当前分组的显隐状态，返回切换后的可见状态。
    pub fn toggle_current_group_visibility(&mut self) -> bool {
        let new_state = !self.group_is_visible(self.active_group);
        self.group_visible.insert(self.active_group, new_state);

        for entry in &mut self.entries {
            if entry.group_id == self.active_group {
                entry.is_visible = new_state;
            }
        }
        new_state
    }

    /// 分组是否处于可见状态；未登记过的分组默认可见。
    fn group_is_visible(&self, group_id: GroupId) -> bool {
        self.group_visible.get(&group_id).copied().unwrap_or(true)
    }

    /// 移除指定 ID 的贴图元数据。
    pub fn remove_paster(&mut self, id: PasterId) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        self.entries.remove(index);
        true
    }

    /// 获取当前活动组的贴图数量
    pub fn count_in_active_group(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.group_id == self.active_group)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paster_grouping_and_toggle() {
        let mut manager = PasterManager::default();
        assert!(manager.add_entry(1.into(), 0.into()));
        assert_eq!(manager.count_in_active_group(), 1);

        manager.switch_group(1.into());
        assert_eq!(manager.count_in_active_group(), 0);
        assert!(manager.add_entry(2.into(), 1.into()));
        assert_eq!(manager.count_in_active_group(), 1);

        manager.switch_group(0.into());
        assert_eq!(manager.count_in_active_group(), 1);

        assert!(!manager.toggle_current_group_visibility());
        assert!(manager.toggle_current_group_visibility());

        assert!(manager.remove_paster(1.into()));
        assert_eq!(manager.count_in_active_group(), 0);
    }

    /// 批量隐藏后新建同组贴图必须继承隐藏态，不能单独显示。
    #[test]
    fn test_new_entry_inherits_hidden_group_state() {
        let mut manager = PasterManager::default();
        assert!(manager.add_entry(1.into(), 0.into()));
        assert!(!manager.toggle_current_group_visibility());

        assert!(manager.add_entry(2.into(), 0.into()));
        let entry = manager
            .entries
            .iter()
            .find(|entry| entry.id == PasterId::from(2))
            .expect("新贴图应登记成功");
        assert!(!entry.is_visible, "隐藏分组中的新贴图不得自动显示");
    }

    #[test]
    fn test_duplicate_id_is_rejected() {
        let mut manager = PasterManager::default();
        assert!(manager.add_entry(7.into(), 0.into()));
        assert!(!manager.add_entry(7.into(), 0.into()));
        assert_eq!(manager.count_in_active_group(), 1);
    }
}
