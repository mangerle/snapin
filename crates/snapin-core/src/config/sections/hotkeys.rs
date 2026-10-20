//! 全局快捷键设置模型定义。

use crate::config::types::Hotkey;
use serde::{Deserialize, Serialize};

/// 全局快捷键设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeyConfig {
    /// 区域截图快捷键
    #[serde(default = "default_snip_hotkey")]
    pub snip: Hotkey,

    /// 贴图快捷键
    #[serde(default = "default_paste_hotkey")]
    pub paste: Hotkey,

    /// 批量隐藏/显示当前组所有贴图
    #[serde(default = "default_toggle_hotkey")]
    pub toggle_pasters: Hotkey,

    /// 切换贴图活动分组快捷键
    #[serde(default = "default_switch_group_hotkey")]
    pub switch_group: Hotkey,
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

/// 从受控常量文本构造内置默认快捷键。
///
/// 默认值全部来自本模块不可变字面量，且均落在 [`Hotkey::parse_str`] 已覆盖测试的合法词表内，
/// 解析失败只可能是代码自身的笔误，故以 expect 在开发期立即暴露，而不是静默退化成禁用状态。
fn default_hotkey(binding: &str) -> Hotkey {
    Hotkey::parse_str(binding).expect("内置默认快捷键必须为合法组合")
}

fn default_snip_hotkey() -> Hotkey {
    default_hotkey("Ctrl+Alt+A")
}

fn default_paste_hotkey() -> Hotkey {
    default_hotkey("F1")
}

fn default_toggle_hotkey() -> Hotkey {
    default_hotkey("Ctrl+F3")
}

fn default_switch_group_hotkey() -> Hotkey {
    default_hotkey("Shift+F3")
}
