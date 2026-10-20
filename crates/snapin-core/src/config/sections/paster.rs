//! 贴图浮动置顶窗口与历史栈设置模型定义。

use crate::config::enums::{DoubleClickAction, ZoomAnchor};
use serde::{Deserialize, Serialize};

/// 贴图窗口设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PasterConfig {
    /// 新贴图初始不透明度 (0.1 ~ 1.0)
    #[serde(default = "default_opacity")]
    pub initial_opacity: f32,

    /// 滚轮缩放时的锚点基准
    #[serde(default)]
    pub zoom_anchor: ZoomAnchor,

    /// 双击贴图窗口时的响应行为
    #[serde(default)]
    pub double_click_action: DoubleClickAction,

    /// 当前激活的贴图分组 ID
    #[serde(default)]
    pub active_group_id: usize,
}

impl Default for PasterConfig {
    fn default() -> Self {
        Self {
            initial_opacity: default_opacity(),
            zoom_anchor: ZoomAnchor::default(),
            double_click_action: DoubleClickAction::default(),
            active_group_id: 0,
        }
    }
}

fn default_opacity() -> f32 {
    1.0
}
