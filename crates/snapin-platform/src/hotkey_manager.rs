//! GNOME 媒体快捷键自动注册与同步管理模块。
//!
//! 在 Ubuntu 26 GNOME 环境下，通过 gsettings 安全配置系统自定义快捷键列表，
//! 将系统按键（如 F1/F3）直接路由至 `snapin snip` 与 `snapin paste` 命令行。

use snapin_core::config::HotkeyConfig;
use snapin_core::error::SnapinError;
use std::process::Command;

/// 自动在 GNOME 设置中注册或更新 Snapin 快捷键
pub fn sync_hotkeys_to_gnome(config: &HotkeyConfig) -> Result<(), SnapinError> {
    tracing::info!("正在向 GNOME 系统注册/同步自定义快捷键...");

    let base_path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
    let snip_path = format!("{base_path}/snapin-snip/");
    let paste_path = format!("{base_path}/snapin-paste/");
    let toggle_path = format!("{base_path}/snapin-toggle/");
    let switch_path = format!("{base_path}/snapin-switch-group/");

    ensure_custom_keybindings_registered(&[&snip_path, &paste_path, &toggle_path, &switch_path])?;

    let current_exe = get_snapin_executable_path();

    setup_keybinding(
        &snip_path,
        "Snapin 区域截图",
        &format!("{current_exe} snip"),
        &config.snip.to_gnome_accelerator(),
    );
    setup_keybinding(
        &paste_path,
        "Snapin 贴图",
        &format!("{current_exe} paste"),
        &config.paste.to_gnome_accelerator(),
    );
    setup_keybinding(
        &toggle_path,
        "Snapin 隐藏/显示贴图",
        &format!("{current_exe} toggle"),
        &config.toggle_pasters.to_gnome_accelerator(),
    );
    setup_keybinding(
        &switch_path,
        "Snapin 切换贴图组",
        &format!("{current_exe} switch-group"),
        &config.switch_group.to_gnome_accelerator(),
    );

    tracing::info!("GNOME 快捷键同步完成");
    Ok(())
}

/// 临时清空 Snapin 在 GNOME 中的快捷键绑定，避免在设置中心录制按键时被系统全局拦截
pub fn suspend_hotkeys_for_settings() -> Result<(), SnapinError> {
    let base_path = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
    let snip_path = format!("{base_path}/snapin-snip/");
    let paste_path = format!("{base_path}/snapin-paste/");
    let toggle_path = format!("{base_path}/snapin-toggle/");
    let switch_path = format!("{base_path}/snapin-switch-group/");

    for path in [&snip_path, &paste_path, &toggle_path, &switch_path] {
        let schema_with_path =
            format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{path}");
        run_gsettings_set(&schema_with_path, "binding", "");
    }
    tracing::info!("已临时挂起 Snapin 全局快捷键，以确保设置中心自由录入按键");
    Ok(())
}

/// 确保指定的自定义按键路径已写入 GNOME custom-keybindings 列表
fn ensure_custom_keybindings_registered(paths: &[&str]) -> Result<(), SnapinError> {
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
        .split('\'')
        .filter(|s| {
            s.starts_with("/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/")
        })
        .map(|s| s.to_string())
        .collect();

    for path in paths {
        if !bindings.iter().any(|b| b == *path) {
            bindings.push((*path).to_string());
        }
    }

    let formatted_list = format!(
        "[{}]",
        bindings
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    run_gsettings_set(
        "org.gnome.settings-daemon.plugins.media-keys",
        "custom-keybindings",
        &formatted_list,
    );

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

fn run_gsettings_set(schema_with_path: &str, key: &str, value: &str) {
    match Command::new("gsettings")
        .args(["set", schema_with_path, key, value])
        .output()
    {
        Ok(out) if !out.status.success() => {
            let err = String::from_utf8_lossy(&out.stderr);
            tracing::warn!("调用 gsettings set 失败 [{key}={value}]: {err}");
        }
        Err(e) => {
            tracing::warn!("无法执行 gsettings 命令: {e}");
        }
        _ => {}
    }
}

fn setup_keybinding(path: &str, name: &str, command: &str, binding: &str) {
    let schema_with_path =
        format!("org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{path}");

    run_gsettings_set(&schema_with_path, "name", name);
    run_gsettings_set(&schema_with_path, "command", command);

    let effective_binding = convert_to_gnome_accelerator(binding);
    run_gsettings_set(&schema_with_path, "binding", &effective_binding);
}

/// 将人类可读的快捷键格式（如 "Ctrl+Alt+A"、"Ctrl+F3"、"F1"）转换为 GNOME / GTK Accelerator 标准格式
/// （如 "<Control><Alt>a"、"<Control>F3"、"F1"）
pub fn convert_to_gnome_accelerator(binding: &str) -> String {
    let trimmed = binding.trim();
    if trimmed.is_empty() || trimmed == "已禁用" || trimmed == "none" {
        return String::new();
    }
    if trimmed.starts_with('<') {
        return trimmed.to_string();
    }
    snapin_core::config::Hotkey::parse_str(trimmed).to_gnome_accelerator()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_to_gnome_accelerator() {
        assert_eq!(convert_to_gnome_accelerator("F1"), "F1");
        assert_eq!(
            convert_to_gnome_accelerator("Ctrl+Alt+A"),
            "<Control><Alt>a"
        );
        assert_eq!(convert_to_gnome_accelerator("Ctrl+F3"), "<Control>F3");
        assert_eq!(convert_to_gnome_accelerator("Shift+F3"), "<Shift>F3");
        assert_eq!(
            convert_to_gnome_accelerator("Ctrl+Shift+S"),
            "<Control><Shift>s"
        );
        assert_eq!(convert_to_gnome_accelerator("Super+A"), "<Super>a");
        assert_eq!(convert_to_gnome_accelerator("已禁用"), "");
        assert_eq!(convert_to_gnome_accelerator(""), "");
    }
}
