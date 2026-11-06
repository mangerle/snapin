//! 跨平台全局热键注册与监听。
//!
//! Linux 通过 GNOME 自定义快捷键将按键路由至命令行；Windows/macOS 使用原生
//! global-hotkey 管理器。两条路径共享 `configured_bindings` 的配置映射与
//! [`HotkeyAction`] 动作契约。

use crate::error::PlatformError;
use snapin_config::config::{Hotkey, HotkeyConfig};

#[cfg(target_os = "linux")]
mod gnome;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native;

/// 跨平台同步系统全局快捷键配置。
///
/// - Linux: 同步至 GNOME 桌面环境媒体快捷键配置；
/// - Windows / macOS: 记录并准备系统级全局热键绑定。
///
/// # Errors
///
/// 原生热键管理器初始化、命令执行或 GNOME 设置同步失败时返回 [`PlatformError`]。
pub fn sync_global_hotkeys(config: &HotkeyConfig) -> Result<(), PlatformError> {
    #[cfg(target_os = "linux")]
    {
        gnome::sync_hotkeys_to_gnome(config)
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        tracing::info!(
            "Windows/macOS 快捷键在守护进程启动时由 PlatformHotkeyListener 自动注册: 截图={}, 贴图={}",
            hotkey_to_accelerator(&config.snip),
            hotkey_to_accelerator(&config.paste)
        );
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = config;
        Ok(())
    }
}

/// 全局热键触发动作枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    /// 触发选区截屏
    Snip,
    /// 触发剪贴板贴图
    Paste,
    /// 触发截屏并直接翻译
    Translate,
    /// 触发批量显隐贴图
    TogglePasters,
    /// 触发切换贴图分组
    SwitchGroup,
    /// 触发智能打码
    SmartRedact,
    /// 触发二维码识别
    Qr,
    /// 触发美化导出
    Beautify,
    /// 显示当前活动分组全部贴图
    Workspace,
    /// 打开或关闭剪贴板历史面板
    ClipboardHistory,
}

/// 一条快捷键绑定：动作与用户配置的按键组合。
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
#[derive(Clone, Copy)]
struct HotkeyBinding<'a> {
    action: HotkeyAction,
    hotkey: &'a Hotkey,
}

/// 统一的快捷键配置到动作映射，供原生监听器与 GNOME 注册共用。
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn configured_bindings(config: &HotkeyConfig) -> [HotkeyBinding<'_>; 10] {
    [
        HotkeyBinding {
            action: HotkeyAction::Snip,
            hotkey: &config.snip,
        },
        HotkeyBinding {
            action: HotkeyAction::Translate,
            hotkey: &config.translate,
        },
        HotkeyBinding {
            action: HotkeyAction::Paste,
            hotkey: &config.paste,
        },
        HotkeyBinding {
            action: HotkeyAction::TogglePasters,
            hotkey: &config.toggle_pasters,
        },
        HotkeyBinding {
            action: HotkeyAction::SwitchGroup,
            hotkey: &config.switch_group,
        },
        HotkeyBinding {
            action: HotkeyAction::SmartRedact,
            hotkey: &config.smart_redact,
        },
        HotkeyBinding {
            action: HotkeyAction::Qr,
            hotkey: &config.qr,
        },
        HotkeyBinding {
            action: HotkeyAction::Beautify,
            hotkey: &config.beautify,
        },
        HotkeyBinding {
            action: HotkeyAction::Workspace,
            hotkey: &config.workspace,
        },
        HotkeyBinding {
            action: HotkeyAction::ClipboardHistory,
            hotkey: &config.clipboard_history,
        },
    ]
}

/// GNOME 自定义快捷键的稳定动作顺序，供注册与挂起复用。
/// 跨平台全局热键监听器
pub struct PlatformHotkeyListener {
    /// 原生热键管理器只承担注册生命周期，动作通过构造时注册的回调回传。
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    _inner: native::NativeHotkeyListener,
}

impl PlatformHotkeyListener {
    /// 初始化系统全局热键监听
    ///
    /// - Linux: 返回无操作空对象（热键由系统设置 media-keys 闭环驱动可执行文件执行）；
    /// - Windows / macOS: 基于 global-hotkey 向操作系统底层注册钩子，并把命中
    ///   动作通过 `on_action` 推送给宿主；回调必须保持非阻塞。
    ///
    /// # Errors
    ///
    /// 原生热键管理器初始化、命令执行或 GNOME 设置同步失败时返回 [`PlatformError`]。
    pub fn new(
        _config: &HotkeyConfig,
        _on_action: impl Fn(HotkeyAction) + Send + Sync + 'static,
    ) -> Result<Self, PlatformError> {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let on_action: std::sync::Arc<dyn Fn(HotkeyAction) + Send + Sync> =
                std::sync::Arc::new(_on_action);
            let inner = native::NativeHotkeyListener::init(_config, on_action)?;
            Ok(Self { _inner: inner })
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Ok(Self {})
        }
    }
}

/// 自动在 GNOME 设置中注册或更新 Snapin 快捷键。
///
/// 快捷键表由 `configured_bindings` 单一维护，GNOME 与原生监听器共享同一份
/// 动作映射，避免两套注册逻辑漏掉或错配某个动作。
///
/// # Errors
///
/// 原生热键管理器初始化、命令执行或 GNOME 设置同步失败时返回 [`PlatformError`]。
pub fn sync_hotkeys_to_gnome(config: &HotkeyConfig) -> Result<(), PlatformError> {
    #[cfg(target_os = "linux")]
    {
        gnome::sync_hotkeys_to_gnome(config)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        Ok(())
    }
}

/// 将热键转换为通用的跨平台按键表达字符串。
///
/// 该表示只供 Windows/macOS 原生热键解析使用；Linux 通过 GNOME accelerator
/// 格式注册，因此该函数仅在原生后端平台编译。
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn hotkey_to_accelerator(hotkey: &Hotkey) -> String {
    if hotkey.is_disabled() {
        return String::new();
    }
    let mut parts = Vec::with_capacity(5);
    if hotkey.ctrl() {
        parts.push("Ctrl");
    }
    if hotkey.alt() {
        parts.push("Alt");
    }
    if hotkey.shift() {
        parts.push("Shift");
    }
    if hotkey.meta() {
        #[cfg(target_os = "macos")]
        parts.push("Cmd");
        #[cfg(not(target_os = "macos"))]
        parts.push("Super");
    }
    parts.push(hotkey.key());
    parts.join("+")
}

/// 将强类型快捷键转换为 GNOME / GTK Accelerator 标准格式（如 `<Control><Alt>a`、"F1"）
pub fn hotkey_to_gnome_accelerator(hk: &Hotkey) -> String {
    if hk.is_disabled() {
        return String::new();
    }

    let mut res = String::new();
    if hk.ctrl() {
        res.push_str("<Control>");
    }
    if hk.alt() {
        res.push_str("<Alt>");
    }
    if hk.shift() {
        res.push_str("<Shift>");
    }
    if hk.meta() {
        res.push_str("<Super>");
    }

    res.push_str(&gnome_key_name(hk.key()));
    res
}

/// 将面向用户直觉的主键展示名换算为 GTK accelerator 键名。
///
/// 展示层使用"Enter / PageUp / Space"这类直观命名，而 GTK 沿用历史键名
/// "Return / Page_Up / space"；两者不一致时 GNOME 会静默忽略整条自定义绑定，
/// 表现为热键录制成功却毫无响应，因此必须在此显式换算。
fn gnome_key_name(key: &str) -> String {
    let named: Option<&'static str> = match key.to_lowercase().as_str() {
        "space" => Some("space"),
        "enter" | "return" => Some("Return"),
        "tab" => Some("Tab"),
        "escape" | "esc" => Some("Escape"),
        "print" | "printscreen" | "sysrq" => Some("Print"),
        "pause" | "break" => Some("Pause"),
        "insert" => Some("Insert"),
        "delete" | "del" => Some("Delete"),
        "home" => Some("Home"),
        "end" => Some("End"),
        "pageup" | "page_up" => Some("Page_Up"),
        "pagedown" | "page_down" => Some("Page_Down"),
        "up" => Some("Up"),
        "down" => Some("Down"),
        "left" => Some("Left"),
        "right" => Some("Right"),
        "menu" => Some("Menu"),
        _ => None,
    };
    if let Some(name) = named {
        return name.to_string();
    }

    // 单字母键按 GTK 约定使用小写形式，其余（数字与符号）原样透传
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => c.to_ascii_lowercase().to_string(),
        _ => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_to_gnome_accelerator() {
        let cases = [
            ("F1", "F1"),
            ("Ctrl+Alt+A", "<Control><Alt>a"),
            ("Ctrl+F3", "<Control>F3"),
            ("Shift+F3", "<Shift>F3"),
            ("Ctrl+Shift+S", "<Control><Shift>s"),
            ("Super+A", "<Super>a"),
            ("Ctrl+Space", "<Control>space"),
            ("Ctrl+Enter", "<Control>Return"),
            ("Alt+PageUp", "<Alt>Page_Up"),
            ("Ctrl+PageDown", "<Control>Page_Down"),
            ("Print", "Print"),
            ("已禁用", ""),
            ("", ""),
        ];
        for (binding, expected) in cases {
            let hotkey =
                Hotkey::parse_str(binding).unwrap_or_else(|e| panic!("{binding} 解析失败: {e}"));
            assert_eq!(
                hotkey_to_gnome_accelerator(&hotkey),
                expected,
                "快捷键 {binding} 的 GNOME accelerator 换算不符"
            );
        }
    }
}

#[cfg(test)]
mod binding_tests {
    use super::*;

    /// 统一映射必须覆盖全部动作，且每个动作只出现一次。
    #[test]
    fn test_configured_bindings_cover_each_action_once() {
        let config = HotkeyConfig::default();
        let bindings = configured_bindings(&config);
        let expected = [
            HotkeyAction::Snip,
            HotkeyAction::Translate,
            HotkeyAction::Paste,
            HotkeyAction::TogglePasters,
            HotkeyAction::SwitchGroup,
            HotkeyAction::SmartRedact,
            HotkeyAction::Qr,
            HotkeyAction::Beautify,
            HotkeyAction::Workspace,
            HotkeyAction::ClipboardHistory,
        ];
        assert_eq!(bindings.len(), expected.len());
        for action in expected {
            let count = bindings
                .iter()
                .filter(|binding| binding.action == action)
                .count();
            assert_eq!(count, 1, "动作 {action:?} 的绑定数量必须为一");
        }
    }
}
