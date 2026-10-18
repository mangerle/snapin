//! GNOME 媒体快捷键自动注册与同步管理模块。
//!
//! 在 Ubuntu 26 GNOME 环境下，通过 gsettings 安全配置系统自定义快捷键列表，
//! 将系统按键（如 F1/F3）直接路由至 `snapin snip` 与 `snapin paste` 命令行。

use crate::config::HotkeyConfig;
use crate::error::SnapinError;
use std::process::Command;

/// 自动在 GNOME 设置中注册或更新 Snapin 快捷键
pub fn sync_hotkeys_to_gnome(config: &HotkeyConfig) -> Result<(), SnapinError> {
    tracing::info!("正在向 GNOME 系统注册/同步自定义快捷键...");

    // 配置基础路径
    let base_path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
    let snip_path = format!("{base_path}/snapin-snip/");
    let paste_path = format!("{base_path}/snapin-paste/");
    let toggle_path = format!("{base_path}/snapin-toggle/");

    // 1. 读取当前已有的自定义快捷键列表
    let output = Command::new("gsettings")
        .args([
            "get",
            "org.gnome.settings-daemon.plugins.media-keys",
            "custom-keybindings",
        ])
        .output()
        .map_err(|e| SnapinError::IoError {
            path: "gsettings".to_string(),
            source: e,
        })?;

    let current_list = String::from_utf8_lossy(&output.stdout);
    let mut bindings: Vec<String> = current_list
        .trim()
        .trim_matches(|c| c == '[' || c == ']' || c == '\n' || c == ' ')
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // 2. 确保 snapin 的三个条目已加入列表
    for path in [&snip_path, &paste_path, &toggle_path] {
        if !bindings.contains(path) {
            bindings.push(path.clone());
        }
    }

    // 将更新后的列表写回 gsettings
    let formatted_list = format!(
        "[{}]",
        bindings
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let _ = Command::new("gsettings")
        .args([
            "set",
            "org.gnome.settings-daemon.plugins.media-keys",
            "custom-keybindings",
            &formatted_list,
        ])
        .output();

    // 3. 配置每个具体快捷键项的名称、执行命令与键位
    let current_exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "snapin".to_string());

    setup_keybinding(
        &snip_path,
        "Snapin 区域截图",
        &format!("{current_exe} snip"),
        &config.snip,
    );

    setup_keybinding(
        &paste_path,
        "Snapin 贴图",
        &format!("{current_exe} paste"),
        &config.paste,
    );

    setup_keybinding(
        &toggle_path,
        "Snapin 隐藏/显示贴图",
        &format!("{current_exe} toggle"),
        &config.toggle_pasters,
    );

    tracing::info!("GNOME 快捷键同步完成");
    Ok(())
}

fn setup_keybinding(path: &str, name: &str, command: &str, binding: &str) {
    let schema_with_path =
        format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{path}");

    let _ = Command::new("gsettings")
        .args(["set", &schema_with_path, "name", name])
        .output();

    let _ = Command::new("gsettings")
        .args(["set", &schema_with_path, "command", command])
        .output();

    let _ = Command::new("gsettings")
        .args(["set", &schema_with_path, "binding", binding])
        .output();
}
