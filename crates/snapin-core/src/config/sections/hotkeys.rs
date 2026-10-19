//! 全局快捷键设置模型定义。

use serde::{Deserialize, Serialize};

/// 全局快捷键设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeyConfig {
    /// 区域截图快捷键
    #[serde(default = "default_snip_hotkey")]
    pub snip: String,

    /// 贴图快捷键
    #[serde(default = "default_paste_hotkey")]
    pub paste: String,

    /// 批量隐藏/显示当前组所有贴图
    #[serde(default = "default_toggle_hotkey")]
    pub toggle_pasters: String,

    /// 切换贴图活动分组快捷键
    #[serde(default = "default_switch_group_hotkey")]
    pub switch_group: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            snip: default_snip_hotkey(),
            paste: default_paste_hotkey(),
            toggle_pasters: default_toggle_hotkey(),
            switch_group: default_switch_group_hotkey(),
        }
    }
}

fn default_snip_hotkey() -> String {
    "Ctrl+Alt+A".to_string()
}

fn default_paste_hotkey() -> String {
    "F1".to_string()
}

fn default_toggle_hotkey() -> String {
    "Ctrl+F3".to_string()
}

fn default_switch_group_hotkey() -> String {
    "Shift+F3".to_string()
}
