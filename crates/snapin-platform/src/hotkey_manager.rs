//! GNOME 媒体快捷键自动注册与同步管理模块。
//!
//! 在 Ubuntu 26 GNOME 环境下，通过 gsettings 安全配置系统自定义快捷键列表，
//! 将系统按键直接路由至 `snapin snip`、`snapin snip --translate` 与 `snapin paste` 等命令行。

use crate::error::PlatformError;
use snapin_core::config::{Hotkey, HotkeyConfig};
#[cfg(target_os = "linux")]
use std::process::Command;

/// 跨平台同步系统全局快捷键配置。
///
/// - Linux: 同步至 GNOME 桌面环境媒体快捷键配置；
/// - Windows / macOS: 记录并准备系统级全局热键绑定。
pub fn sync_global_hotkeys(config: &HotkeyConfig) -> Result<(), PlatformError> {
    #[cfg(target_os = "linux")]
    {
        sync_hotkeys_to_gnome(config)
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
fn configured_bindings(config: &HotkeyConfig) -> [HotkeyBinding<'_>; 5] {
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
    ]
}

/// GNOME 自定义快捷键的稳定动作顺序，供注册与挂起复用。
#[cfg(target_os = "linux")]
const HOTKEY_ACTIONS: [HotkeyAction; 5] = [
    HotkeyAction::Snip,
    HotkeyAction::Translate,
    HotkeyAction::Paste,
    HotkeyAction::TogglePasters,
    HotkeyAction::SwitchGroup,
];

/// 返回 GNOME 注册所需的路径后缀、显示名与命令行参数。
#[cfg(target_os = "linux")]
fn gnome_hotkey_metadata(
    action: HotkeyAction,
) -> (&'static str, &'static str, &'static [&'static str]) {
    match action {
        HotkeyAction::Snip => ("snapin-snip", "Snapin 区域截图", &["snip"]),
        HotkeyAction::Translate => (
            "snapin-translate",
            "Snapin 截图翻译",
            &["snip", "--translate"],
        ),
        HotkeyAction::Paste => ("snapin-paste", "Snapin 贴图", &["paste"]),
        HotkeyAction::TogglePasters => ("snapin-toggle", "Snapin 隐藏/显示贴图", &["toggle"]),
        HotkeyAction::SwitchGroup => (
            "snapin-switch-group",
            "Snapin 切换贴图组",
            &["switch-group"],
        ),
    }
}

/// 跨平台全局热键监听器
pub struct PlatformHotkeyListener {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    inner: native_hotkey::NativeHotkeyListener,
}

impl PlatformHotkeyListener {
    /// 初始化系统全局热键监听
    ///
    /// - Linux: 返回无操作空对象（热键由系统设置 media-keys 闭环驱动可执行文件执行）；
    /// - Windows / macOS: 基于 global-hotkey 向操作系统底层注册钩子。
    pub fn new(_config: &HotkeyConfig) -> Result<Self, PlatformError> {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let inner = native_hotkey::NativeHotkeyListener::init(_config)?;
            Ok(Self { inner })
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Ok(Self {})
        }
    }

    /// 非阻塞检查是否按下了注册的全局快捷键
    pub fn poll_action(&self) -> Option<HotkeyAction> {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.inner.try_recv()
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            None
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native_hotkey {
    use super::{hotkey_to_accelerator, HotkeyAction};
    use crate::error::PlatformError;
    use global_hotkey::hotkey::HotKey;
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
    use snapin_core::config::HotkeyConfig;
    use std::collections::HashMap;

    pub struct NativeHotkeyListener {
        _manager: GlobalHotKeyManager,
        action_map: HashMap<u32, HotkeyAction>,
    }

    impl NativeHotkeyListener {
        pub fn init(config: &HotkeyConfig) -> Result<Self, PlatformError> {
            let manager = GlobalHotKeyManager::new().map_err(|e| PlatformError::Hotkey {
                message: format!("初始化原生全局热键管理器失败: {e}"),
            })?;

            let bindings = super::configured_bindings(config);
            let mut action_map = HashMap::with_capacity(bindings.len());

            for binding in bindings {
                let acc = hotkey_to_accelerator(binding.hotkey);
                if let Ok(hk) = acc.parse::<HotKey>() {
                    if let Err(e) = manager.register(hk) {
                        tracing::warn!("注册快捷键 [{acc}] 失败: {e}");
                    } else {
                        action_map.insert(hk.id, binding.action);
                    }
                }
            }

            Ok(Self {
                _manager: manager,
                action_map,
            })
        }

        pub fn try_recv(&self) -> Option<HotkeyAction> {
            while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                if event.state == HotKeyState::Pressed {
                    if let Some(&action) = self.action_map.get(&event.id) {
                        return Some(action);
                    }
                }
            }
            None
        }
    }
}

/// 将热键转换为通用的跨平台按键表达字符串
pub fn hotkey_to_accelerator(hotkey: &Hotkey) -> String {
    let mut parts = Vec::new();
    if hotkey.ctrl {
        parts.push("Ctrl");
    }
    if hotkey.alt {
        parts.push("Alt");
    }
    if hotkey.shift {
        parts.push("Shift");
    }
    if hotkey.meta {
        #[cfg(target_os = "macos")]
        parts.push("Cmd");
        #[cfg(not(target_os = "macos"))]
        parts.push("Super");
    }
    parts.push(hotkey.key.as_str());
    parts.join("+")
}

/// 自动在 GNOME 设置中注册或更新 Snapin 快捷键。
///
/// 快捷键表由 [`configured_bindings`] 单一维护，GNOME 与原生监听器共享同一份
/// 动作映射，避免两套注册逻辑漏掉或错配某个动作。
pub fn sync_hotkeys_to_gnome(config: &HotkeyConfig) -> Result<(), PlatformError> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = config;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        tracing::info!("正在向 GNOME 系统注册/同步自定义快捷键...");

        let base_path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
        let bindings = configured_bindings(config);
        let paths: Vec<String> = bindings
            .iter()
            .map(|binding| format!("{base_path}/{}/", gnome_hotkey_metadata(binding.action).0))
            .collect();
        ensure_custom_keybindings_registered(&paths)?;

        let current_exe = get_snapin_executable_path();
        let mut batch_items: Vec<(String, String, String)> = Vec::with_capacity(bindings.len() * 3);
        for binding in bindings {
            let (suffix, name, args) = gnome_hotkey_metadata(binding.action);
            let schema = format!(
                "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{base_path}/{suffix}/"
            );
            let command = format!("{current_exe} {}", args.join(" "));
            let binding_text = hotkey_to_gnome_accelerator(binding.hotkey);
            batch_items.push((schema.clone(), "name".to_string(), name.to_string()));
            batch_items.push((schema.clone(), "command".to_string(), command));
            batch_items.push((schema, "binding".to_string(), binding_text));
        }

        let batch_refs: Vec<(&str, &str, &str)> = batch_items
            .iter()
            .map(|(schema, key, value)| (schema.as_str(), key.as_str(), value.as_str()))
            .collect();
        execute_gsettings_batch(&batch_refs);

        tracing::info!("GNOME 快捷键同步完成");
        Ok(())
    }
}

/// 临时清空 Snapin 在 GNOME 中的快捷键绑定，避免在设置中心录制按键时被系统全局拦截
pub fn suspend_hotkeys_for_settings() -> Result<(), PlatformError> {
    #[cfg(not(target_os = "linux"))]
    {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let base_path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
        let schemas: Vec<String> = HOTKEY_ACTIONS
            .iter()
            .map(|action| {
                format!(
                    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{base_path}/{}/",
                    gnome_hotkey_metadata(*action).0
                )
            })
            .collect();

        let items: Vec<(&str, &str, &str)> = schemas
            .iter()
            .map(|s| (s.as_str(), "binding", ""))
            .collect();

        execute_gsettings_batch(&items);
        tracing::info!("已临时挂起 Snapin 全局快捷键，以确保设置中心自由录入按键");
        Ok(())
    }
}

/// 确保指定的自定义按键路径已写入 GNOME custom-keybindings 列表
#[cfg(target_os = "linux")]
fn ensure_custom_keybindings_registered(paths: &[String]) -> Result<(), PlatformError> {
    let output = Command::new("gsettings")
        .args([
            "get",
            "org.gnome.settings-daemon.plugins.media-keys",
            "custom-keybindings",
        ])
        .output()
        .map_err(|e| PlatformError::Io {
            path: "gsettings".to_string(),
            source: e,
        })?;

    let current_list = String::from_utf8_lossy(&output.stdout);
    let mut bindings: Vec<String> = current_list
        .split('\'')
        .filter(|s| {
            s.starts_with("/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/")
        })
        .map(|s| s.to_string())
        .collect();

    let mut changed = false;
    for path in paths {
        if !bindings.iter().any(|binding| binding == path) {
            bindings.push(path.clone());
            changed = true;
        }
    }

    if changed {
        let formatted_list = format!(
            "[{}]",
            bindings
                .iter()
                .map(|s| format!("'{s}'"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        execute_gsettings_batch(&[(
            "org.gnome.settings-daemon.plugins.media-keys",
            "custom-keybindings",
            &formatted_list,
        )]);
    }

    Ok(())
}

/// 获取 Snapin 当前或系统安装的可执行文件路径
#[cfg(target_os = "linux")]
fn get_snapin_executable_path() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if exe.exists() {
            return exe.to_string_lossy().to_string();
        }
    }
    if std::path::Path::new("/usr/bin/snapin").exists() {
        "/usr/bin/snapin".to_string()
    } else {
        "snapin".to_string()
    }
}

/// 批量执行 gsettings set，逐项直接以参数数组调用可执行文件。
///
/// 旧实现把若干条命令拼接成脚本交给 `sh -c` 执行，虽然对单引号做了转义，但仍让
/// 用户可编辑的快捷键文本与可执行路径进入 shell 解析上下文，属于无必要的命令注入面。
/// gsettings 单次调用本身仅毫秒级，改为直接传参既消除注入面又保持行为一致。
#[cfg(target_os = "linux")]
fn execute_gsettings_batch(items: &[(&str, &str, &str)]) {
    for (schema, key, value) in items {
        match Command::new("gsettings")
            .args(["set", schema, key, value])
            .output()
        {
            Ok(out) if !out.status.success() => {
                let err = String::from_utf8_lossy(&out.stderr);
                tracing::warn!("调用 gsettings set 失败 [{schema} {key}={value}]: {err}");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!("无法执行 gsettings 命令，原因: {error}");
            }
        }
    }
}

/// 将强类型快捷键转换为 GNOME / GTK Accelerator 标准格式（如 "<Control><Alt>a"、"F1"）
pub fn hotkey_to_gnome_accelerator(hk: &Hotkey) -> String {
    if hk.key.is_empty() {
        return String::new();
    }

    let mut res = String::new();
    if hk.ctrl {
        res.push_str("<Control>");
    }
    if hk.alt {
        res.push_str("<Alt>");
    }
    if hk.shift {
        res.push_str("<Shift>");
    }
    if hk.meta {
        res.push_str("<Super>");
    }

    res.push_str(&gnome_key_name(&hk.key));
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

    /// GNOME 自定义快捷键路径后缀必须唯一，否则后注册项会覆盖前一项。
    #[cfg(target_os = "linux")]
    #[test]
    fn test_gnome_hotkey_suffixes_are_unique() {
        let mut suffixes = Vec::new();
        for action in HOTKEY_ACTIONS {
            let (suffix, _, _) = gnome_hotkey_metadata(action);
            assert!(
                !suffixes.contains(&suffix),
                "GNOME 快捷键后缀重复: {suffix}"
            );
            suffixes.push(suffix);
        }
    }
}
