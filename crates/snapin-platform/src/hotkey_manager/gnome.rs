//! Linux GNOME 自定义快捷键注册与挂起。

use super::{configured_bindings, hotkey_to_gnome_accelerator, HotkeyAction};
use crate::error::PlatformError;
use snapin_config::config::HotkeyConfig;
use std::process::Command;

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
        HotkeyAction::SmartRedact => (
            "snapin-smart-redact",
            "Snapin 智能打码",
            &["snip", "--redact"],
        ),
        HotkeyAction::Qr => ("snapin-qr", "Snapin 二维码识别", &["snip", "--qr"]),
        HotkeyAction::Beautify => (
            "snapin-beautify",
            "Snapin 美化导出",
            &["snip", "--beautify"],
        ),
        HotkeyAction::Workspace => ("snapin-workspace", "Snapin 贴图工作区", &["workspace"]),
        HotkeyAction::ClipboardHistory => (
            "snapin-clipboard-history",
            "Snapin 剪贴板历史",
            &["clipboard-history"],
        ),
    }
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
            let command = format!("{} {}", quote_shell_arg(&current_exe), args.join(" "));
            let binding_text = hotkey_to_gnome_accelerator(binding.hotkey);
            batch_items.push((schema.clone(), "name".to_string(), name.to_string()));
            batch_items.push((schema.clone(), "command".to_string(), command));
            batch_items.push((schema, "binding".to_string(), binding_text));
        }

        let batch_refs: Vec<(&str, &str, &str)> = batch_items
            .iter()
            .map(|(schema, key, value)| (schema.as_str(), key.as_str(), value.as_str()))
            .collect();
        execute_gsettings_batch(&batch_refs)?;

        tracing::info!("GNOME 快捷键同步完成");
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
    if !output.status.success() {
        return Err(PlatformError::Hotkey {
            message: format!(
                "读取 GNOME custom-keybindings 失败: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }

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
        )])?;
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
fn execute_gsettings_batch(items: &[(&str, &str, &str)]) -> Result<(), PlatformError> {
    let mut failures: Vec<String> = Vec::new();
    for (schema, key, value) in items {
        match Command::new("gsettings")
            .args(["set", schema, key, value])
            .output()
        {
            Ok(out) if !out.status.success() => {
                failures.push(format!(
                    "{schema} {key}: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            Ok(_) => {}
            Err(error) => failures.push(format!("{schema} {key}: {error}")),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(PlatformError::Hotkey {
            message: format!("写入 GNOME 快捷键设置失败: {}", failures.join("；")),
        })
    }
}

/// 给含空白的可执行路径加 shell 引号，避免 GNOME 解析 command 时被拆成多个参数。
#[cfg(target_os = "linux")]
fn quote_shell_arg(value: &str) -> String {
    let safe = value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'));
    if safe {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试覆盖的稳定动作顺序。
    const HOTKEY_ACTIONS: [HotkeyAction; 10] = [
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

    /// 含空格的路径必须加引用，单引号需按 POSIX 规则转义。
    #[test]
    fn test_quote_shell_arg_for_command_path() {
        assert_eq!(quote_shell_arg("/usr/bin/snapin"), "/usr/bin/snapin");
        assert_eq!(
            quote_shell_arg("/home/my tools/snapin"),
            "'/home/my tools/snapin'"
        );
        assert_eq!(
            quote_shell_arg("/home/a'b/snapin"),
            "'/home/a'\\''b/snapin'"
        );
    }

    /// GNOME 自定义快捷键路径后缀必须唯一，否则后注册项会覆盖前一项。
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
