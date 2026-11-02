//! 全局快捷键设置模型定义。

use crate::config::types::Hotkey;
use serde::{Deserialize, Serialize};

/// 全局快捷键设置板块
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeyConfig {
    /// 区域截图快捷键
    #[serde(default = "default_snip_hotkey")]
    pub snip: Hotkey,

    /// 截图后自动翻译快捷键
    #[serde(default = "default_translate_hotkey")]
    pub translate: Hotkey,

    /// 贴图快捷键
    #[serde(default = "default_paste_hotkey")]
    pub paste: Hotkey,

    /// 批量隐藏/显示当前组所有贴图
    #[serde(default = "default_toggle_hotkey")]
    pub toggle_pasters: Hotkey,

    /// 切换贴图活动分组快捷键
    #[serde(default = "default_switch_group_hotkey")]
    pub switch_group: Hotkey,

    /// 智能打码快捷键
    #[serde(default = "default_smart_redact_hotkey")]
    pub smart_redact: Hotkey,

    /// 二维码识别快捷键
    #[serde(default = "default_qr_hotkey")]
    pub qr: Hotkey,

    /// 美化导出快捷键
    #[serde(default = "default_beautify_hotkey")]
    pub beautify: Hotkey,

    /// 贴图工作区面板快捷键
    #[serde(default = "default_workspace_hotkey")]
    pub workspace: Hotkey,

    /// 剪贴板历史面板快捷键
    #[serde(default = "default_clipboard_history_hotkey")]
    pub clipboard_history: Hotkey,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            snip: default_snip_hotkey(),
            translate: default_translate_hotkey(),
            paste: default_paste_hotkey(),
            toggle_pasters: default_toggle_hotkey(),
            switch_group: default_switch_group_hotkey(),
            smart_redact: default_smart_redact_hotkey(),
            qr: default_qr_hotkey(),
            beautify: default_beautify_hotkey(),
            workspace: default_workspace_hotkey(),
            clipboard_history: default_clipboard_history_hotkey(),
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

fn default_translate_hotkey() -> Hotkey {
    default_hotkey("Ctrl+Alt+D")
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

fn default_smart_redact_hotkey() -> Hotkey {
    default_hotkey("Ctrl+Alt+R")
}

fn default_qr_hotkey() -> Hotkey {
    default_hotkey("Ctrl+Alt+Q")
}

fn default_beautify_hotkey() -> Hotkey {
    default_hotkey("Ctrl+Alt+B")
}

fn default_workspace_hotkey() -> Hotkey {
    default_hotkey("Ctrl+Alt+W")
}

fn default_clipboard_history_hotkey() -> Hotkey {
    default_hotkey("Ctrl+Shift+V")
}
