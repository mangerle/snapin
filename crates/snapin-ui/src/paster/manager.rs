//! 贴图多窗口生命周期调度与多分组隔离管理器。
//!
//! 管理器只维护贴图 ID、分组与可见性等轻量元数据，不持有像素缓冲区，
//! 像素数据由各贴图窗口独占，避免每张图在窗口与管理器之间发生多次深拷贝。

use crate::paster::transform::PasterTransform;
use tiny_skia::Pixmap;

/// 单张贴图的数据上下文（像素数据由所属窗口独占）
#[derive(Debug)]
pub struct PasterItem {
    pub id: u64,
    pub title: String,
    pub pixmap: Pixmap,
    pub position: (i32, i32),
    pub transform: PasterTransform,
    pub is_visible: bool,
    pub group_id: usize,
}

impl PasterItem {
    /// 构造新的贴图图元项
    pub fn new(id: u64, title: String, pixmap: Pixmap, position: (i32, i32)) -> Self {
        Self {
            id,
            title,
            pixmap,
            position,
            transform: PasterTransform::default(),
            is_visible: true,
            group_id: 0,
        }
    }
}

/// 单张贴图的元数据条目
#[derive(Debug, Clone, PartialEq, Eq)]
struct PasterEntry {
    id: u64,
    group_id: usize,
    is_visible: bool,
}

/// 贴图分组管理器
#[derive(Default)]
pub struct PasterManager {
    active_group: usize,
    entries: Vec<PasterEntry>,
}

impl PasterManager {
    /// 获取当前活动分组 ID
    pub fn active_group(&self) -> usize {
        self.active_group
    }

    /// 登记一张新贴图；ID 重复时返回 `false`，可见性由当前活动分组决定。
    pub fn add_entry(&mut self, id: u64, group_id: usize) -> bool {
        if self.entries.iter().any(|entry| entry.id == id) {
            return false;
        }

        self.entries.push(PasterEntry {
            id,
            group_id,
            is_visible: group_id == self.active_group,
        });
        true
    }

    /// 切换当前活动分组，并同步组内贴图可见性。
    pub fn switch_group(&mut self, group_id: usize) {
        self.active_group = group_id;
        for entry in &mut self.entries {
            entry.is_visible = entry.group_id == group_id;
        }
    }

    /// 批量切换当前分组内所有贴图的显示/隐藏状态，返回切换后的可见状态。
    pub fn toggle_current_group_visibility(&mut self) -> bool {
        let any_visible = self
            .entries
            .iter()
            .any(|entry| entry.group_id == self.active_group && entry.is_visible);
        let new_state = !any_visible;

        for entry in &mut self.entries {
            if entry.group_id == self.active_group {
                entry.is_visible = new_state;
            }
        }
        new_state
    }

    /// 移除指定 ID 的贴图元数据。
    pub fn remove_paster(&mut self, id: u64) -> bool {
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
        assert!(manager.add_entry(1, 0));
        assert_eq!(manager.count_in_active_group(), 1);

        manager.switch_group(1);
        assert_eq!(manager.count_in_active_group(), 0);
        assert!(manager.add_entry(2, 1));
        assert_eq!(manager.count_in_active_group(), 1);

        manager.switch_group(0);
        assert_eq!(manager.count_in_active_group(), 1);

        assert!(!manager.toggle_current_group_visibility());
        assert!(manager.toggle_current_group_visibility());

        assert!(manager.remove_paster(1));
        assert_eq!(manager.count_in_active_group(), 0);
    }

    #[test]
    fn test_duplicate_id_is_rejected() {
        let mut manager = PasterManager::default();
        assert!(manager.add_entry(7, 0));
        assert!(!manager.add_entry(7, 0));
        assert_eq!(manager.count_in_active_group(), 1);
    }
}
