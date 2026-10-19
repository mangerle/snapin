//! 贴图多窗口生命周期调度与多分组隔离管理器。
//!
//! 维护活动贴图列表、历史缓存栈、多工作区分组以及批量显示隐藏状态。

use crate::paster::transform::PasterTransform;
use std::collections::HashMap;
use tiny_skia::Pixmap;

/// 单张贴图的数据上下文
#[derive(Debug, Clone)]
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

/// 贴图分组管理器
pub struct PasterManager {
    next_id: u64,
    active_group: usize,
    items: Vec<PasterItem>,
    group_names: HashMap<usize, String>,
    max_history: usize,
}

impl Default for PasterManager {
    fn default() -> Self {
        let mut group_names = HashMap::new();
        group_names.insert(0, "默认分组".to_string());
        Self {
            next_id: 1,
            active_group: 0,
            items: Vec::new(),
            group_names,
            max_history: 20,
        }
    }
}

impl PasterManager {
    /// 创建并添加一张新贴图
    pub fn add_paster(&mut self, pixmap: Pixmap, pos: (i32, i32)) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let title = format!("Snapin_Paster_{id}");

        let item = PasterItem {
            id,
            title,
            pixmap,
            position: pos,
            transform: PasterTransform::default(),
            is_visible: true,
            group_id: self.active_group,
        };

        self.items.push(item);
        id
    }

    /// 获取当前活动分组 ID
    pub fn active_group(&self) -> usize {
        self.active_group
    }

    /// 获取指定分组的展示名称
    pub fn group_name(&self, id: usize) -> Option<&str> {
        self.group_names.get(&id).map(|s| s.as_str())
    }

    /// 获取最大历史记录上限
    pub fn max_history(&self) -> usize {
        self.max_history
    }

    /// 切换当前活动分组
    pub fn switch_group(&mut self, group_id: usize) {
        self.active_group = group_id;
        // 同步更新各贴图可见性：仅显示属于当前组的贴图
        for item in &mut self.items {
            item.is_visible = item.group_id == group_id;
        }
    }

    /// 批量切换当前分组内所有贴图的显示/隐藏状态
    pub fn toggle_current_group_visibility(&mut self) -> bool {
        let any_visible = self
            .items
            .iter()
            .any(|it| it.group_id == self.active_group && it.is_visible);
        let new_state = !any_visible;

        for item in &mut self.items {
            if item.group_id == self.active_group {
                item.is_visible = new_state;
            }
        }
        new_state
    }

    /// 移除指定 ID 的贴图
    pub fn remove_paster(&mut self, id: u64) -> Option<PasterItem> {
        if let Some(idx) = self.items.iter().position(|it| it.id == id) {
            Some(self.items.remove(idx))
        } else {
            None
        }
    }

    /// 注册外部运行的贴图实例元数据
    pub fn register_instance(&mut self, id: u64, group_id: usize) {
        if !self.items.iter().any(|it| it.id == id) {
            let item = PasterItem {
                id,
                title: format!("Snapin_Paster_{id}"),
                pixmap: Pixmap::new(1, 1).unwrap(),
                position: (0, 0),
                transform: PasterTransform::default(),
                is_visible: group_id == self.active_group,
                group_id,
            };
            self.items.push(item);
        }
    }

    /// 注销贴图实例
    pub fn unregister_instance(&mut self, id: u64) -> bool {
        if let Some(idx) = self.items.iter().position(|it| it.id == id) {
            self.items.remove(idx);
            true
        } else {
            false
        }
    }

    /// 获取当前活动组的贴图数量
    pub fn count_in_active_group(&self) -> usize {
        self.items
            .iter()
            .filter(|it| it.group_id == self.active_group)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paster_grouping_and_toggle() {
        let mut mgr = PasterManager::default();
        let dummy = Pixmap::new(10, 10).unwrap();

        let id1 = mgr.add_paster(dummy.clone(), (0, 0));
        assert_eq!(mgr.count_in_active_group(), 1);

        // 切换到分组 1 并添加贴图
        mgr.switch_group(1);
        assert_eq!(mgr.count_in_active_group(), 0);
        let _id2 = mgr.add_paster(dummy.clone(), (10, 10));
        assert_eq!(mgr.count_in_active_group(), 1);

        // 切换回默认分组 0
        mgr.switch_group(0);
        assert_eq!(mgr.count_in_active_group(), 1);

        // 批量切换可见性
        let state = mgr.toggle_current_group_visibility();
        assert!(!state); // 变为全部隐藏
        let state = mgr.toggle_current_group_visibility();
        assert!(state); // 变为全部显示

        mgr.remove_paster(id1);
        assert_eq!(mgr.count_in_active_group(), 0);
    }

    #[test]
    fn test_paster_instance_registration() {
        let mut mgr = PasterManager::default();
        mgr.register_instance(101, 0);
        assert_eq!(mgr.count_in_active_group(), 1);

        // 注册到非活动组
        mgr.register_instance(102, 1);
        assert_eq!(mgr.count_in_active_group(), 1);

        // 切换组
        mgr.switch_group(1);
        assert_eq!(mgr.count_in_active_group(), 1);

        // 注销实例
        assert!(mgr.unregister_instance(102));
        assert_eq!(mgr.count_in_active_group(), 0);
    }
}
