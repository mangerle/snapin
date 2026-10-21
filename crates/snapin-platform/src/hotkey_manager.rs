//! GNOME 媒体快捷键自动注册与同步管理模块。
//!
//! 在 Ubuntu 26 GNOME 环境下，通过 gsettings 安全配置系统自定义快捷键列表，
//! 将系统按键直接路由至 `snapin snip`、`snapin snip --translate` 与 `snapin paste` 等命令行。

use crate::error::PlatformError;
use snapin_core::config::{Hotkey, HotkeyConfig};
use std::process::Command;

/// 自动在 GNOME 设置中注册或更新 Snapin 快捷键
pub fn sync_hotkeys_to_gnome(config: &HotkeyConfig) -> Result<(), PlatformError> {
    tracing::info!("正在向 GNOME 系统注册/同步自定义快捷键...");

    let base_path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
    let snip_path = format!("{base_path}/snapin-snip/");
    let translate_path = format!("{base_path}/snapin-translate/");
    let paste_path = format!("{base_path}/snapin-paste/");
    let toggle_path = format!("{base_path}/snapin-toggle/");
    let switch_path = format!("{base_path}/snapin-switch-group/");

    ensure_custom_keybindings_registered(&[
        &snip_path,
        &translate_path,
        &paste_path,
        &toggle_path,
        &switch_path,
    ])?;

    let current_exe = get_snapin_executable_path();

    let snip_schema =
        format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{snip_path}");
    let snip_cmd = format!("{current_exe} snip");
    let snip_binding = hotkey_to_gnome_accelerator(&config.snip);

    let translate_schema =
        format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{translate_path}");
    let translate_cmd = format!("{current_exe} snip --translate");
    let translate_binding = hotkey_to_gnome_accelerator(&config.translate);

    let paste_schema =
        format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{paste_path}");
    let paste_cmd = format!("{current_exe} paste");
    let paste_binding = hotkey_to_gnome_accelerator(&config.paste);

    let toggle_schema =
        format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{toggle_path}");
    let toggle_cmd = format!("{current_exe} toggle");
    let toggle_binding = hotkey_to_gnome_accelerator(&config.toggle_pasters);

    let switch_schema =
        format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{switch_path}");
    let switch_cmd = format!("{current_exe} switch-group");
    let switch_binding = hotkey_to_gnome_accelerator(&config.switch_group);

    let batch_items = [
        (snip_schema.as_str(), "name", "Snapin 区域截图"),
        (snip_schema.as_str(), "command", snip_cmd.as_str()),
        (snip_schema.as_str(), "binding", snip_binding.as_str()),
        (translate_schema.as_str(), "name", "Snapin 截图翻译"),
        (translate_schema.as_str(), "command", translate_cmd.as_str()),
        (
            translate_schema.as_str(),
            "binding",
            translate_binding.as_str(),
        ),
        (paste_schema.as_str(), "name", "Snapin 贴图"),
        (paste_schema.as_str(), "command", paste_cmd.as_str()),
        (paste_schema.as_str(), "binding", paste_binding.as_str()),
        (toggle_schema.as_str(), "name", "Snapin 隐藏/显示贴图"),
        (toggle_schema.as_str(), "command", toggle_cmd.as_str()),
        (toggle_schema.as_str(), "binding", toggle_binding.as_str()),
        (switch_schema.as_str(), "name", "Snapin 切换贴图组"),
        (switch_schema.as_str(), "command", switch_cmd.as_str()),
        (switch_schema.as_str(), "binding", switch_binding.as_str()),
    ];

    execute_gsettings_batch(&batch_items);

    tracing::info!("GNOME 快捷键同步完成");
    Ok(())
}

/// 临时清空 Snapin 在 GNOME 中的快捷键绑定，避免在设置中心录制按键时被系统全局拦截
pub fn suspend_hotkeys_for_settings() -> Result<(), PlatformError> {
    let base_path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
    let schemas = [
        format!(
            "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{base_path}/snapin-snip/"
        ),
        format!(
            "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{base_path}/snapin-translate/"
        ),
        format!(
            "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{base_path}/snapin-paste/"
        ),
        format!(
            "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{base_path}/snapin-toggle/"
        ),
        format!(
            "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{base_path}/snapin-switch-group/"
        ),
    ];

    let items: Vec<(&str, &str, &str)> = schemas
        .iter()
        .map(|s| (s.as_str(), "binding", ""))
        .collect();

    execute_gsettings_batch(&items);
    tracing::info!("已临时挂起 Snapin 全局快捷键，以确保设置中心自由录入按键");
    Ok(())
}

/// 确保指定的自定义按键路径已写入 GNOME custom-keybindings 列表
fn ensure_custom_keybindings_registered(paths: &[&str]) -> Result<(), PlatformError> {
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
        if !bindings.iter().any(|b| b == *path) {
            bindings.push((*path).to_string());
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
