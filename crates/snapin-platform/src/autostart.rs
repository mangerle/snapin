//! 跨平台开机自启动管理。
//!
//! - Linux: systemd 用户服务优先，XDG autostart 桌面项兜底；
//! - Windows: 注册表当前用户启动项 (HKCU\Software\Microsoft\Windows\CurrentVersion\Run)；
//! - macOS: 用户级 LaunchAgents 服务代理清单 (~/Library/LaunchAgents/local.wangle.snapin.plist)。

use crate::error::PlatformError;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::time::Duration;

/// 自启动相关系统命令的最长执行时间。
#[cfg(any(target_os = "linux", target_os = "windows"))]
const AUTOSTART_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// 转义写入 XML 文本节点的保留字符。
#[cfg(any(target_os = "macos", test))]
fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// 同步开机自启动配置状态。
///
/// 针对不同操作系统自动应用对应的原生自启持久化通路。
///
/// # Errors
///
/// 目录、命令、符号链接或平台自启动配置失败时返回 [`PlatformError`]。
pub fn sync_autostart(enabled: bool) -> Result<(), PlatformError> {
    #[cfg(target_os = "linux")]
    {
        linux::sync_autostart_linux(enabled)
    }

    #[cfg(target_os = "windows")]
    {
        windows::sync_autostart_windows(enabled)
    }

    #[cfg(target_os = "macos")]
    {
        macos::sync_autostart_macos(enabled)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = enabled;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use crate::error::PlatformError;
    use crate::output_with_timeout;
    use std::path::PathBuf;
    use std::process::Command;

    const DESKTOP_ENTRY_CONTENT: &str = r#"[Desktop Entry]
Type=Application
Name=Snapin
Comment=Snapin 截图与贴图守护服务
Exec=snapin daemon
Terminal=false
Categories=Utility;
X-GNOME-Autostart-enabled=true
"#;

    const SYSTEMD_SERVICE: &str = "snapin";

    pub fn sync_autostart_linux(enabled: bool) -> Result<(), PlatformError> {
        if systemd_unit_loaded() {
            sync_systemd_service(enabled)?;
            remove_desktop_file()?;
            return Ok(());
        }

        sync_desktop_file(enabled)
    }

    fn systemd_unit_loaded() -> bool {
        let mut command = Command::new("systemctl");
        command.args([
            "--user",
            "show",
            "-p",
            "LoadState",
            "--value",
            SYSTEMD_SERVICE,
        ]);
        let output = match output_with_timeout(command, super::AUTOSTART_COMMAND_TIMEOUT) {
            Ok(output) => output,
            Err(error) => {
                tracing::debug!(
                    "无法执行 systemctl 探测 systemd 单元，回退 XDG autostart: {error}"
                );
                return false;
            }
        };

        if !output.status.success() {
            return false;
        }

        String::from_utf8_lossy(&output.stdout).trim() == "loaded"
    }

    fn sync_systemd_service(enabled: bool) -> Result<(), PlatformError> {
        let action = if enabled { "enable" } else { "disable" };
        let mut command = Command::new("systemctl");
        command.args(["--user", action, SYSTEMD_SERVICE]);
        let output =
            output_with_timeout(command, super::AUTOSTART_COMMAND_TIMEOUT).map_err(|error| {
                PlatformError::Systemd {
                    action: action.to_string(),
                    service: SYSTEMD_SERVICE.to_string(),
                    message: error.to_string(),
                }
            })?;

        if !output.status.success() {
            return Err(PlatformError::Systemd {
                action: action.to_string(),
                service: SYSTEMD_SERVICE.to_string(),
                message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        tracing::info!(
            "已{} systemd 用户服务开机自启动: {}",
            if enabled { "启用" } else { "停用" },
            SYSTEMD_SERVICE
        );
        Ok(())
    }

    fn sync_desktop_file(enabled: bool) -> Result<(), PlatformError> {
        let Some(desktop_file) = desktop_file_path() else {
            return Ok(());
        };

        if enabled {
            let Some(autostart_dir) = desktop_file.parent() else {
                return Ok(());
            };
            std::fs::create_dir_all(autostart_dir).map_err(|error| PlatformError::Io {
                path: autostart_dir.to_string_lossy().to_string(),
                source: error,
            })?;
            let temp_path = autostart_dir.join(format!(
                ".snapin.{}.tmp",
                snapin_foundation::AppPaths::generate_unique_id()
            ));
            if let Err(error) = snapin_foundation::AppPaths::write_secure_file(
                &temp_path,
                DESKTOP_ENTRY_CONTENT.as_bytes(),
            ) {
                let _ = std::fs::remove_file(&temp_path);
                return Err(PlatformError::Io {
                    path: temp_path.to_string_lossy().to_string(),
                    source: error,
                });
            }
            if let Err(error) =
                snapin_foundation::AppPaths::replace_file_atomically(&temp_path, &desktop_file)
            {
                let _ = std::fs::remove_file(&temp_path);
                return Err(PlatformError::Io {
                    path: desktop_file.to_string_lossy().to_string(),
                    source: error,
                });
            }
        } else {
            remove_desktop_file()?;
        }

        Ok(())
    }

    fn remove_desktop_file() -> Result<(), PlatformError> {
        let Some(desktop_file) = desktop_file_path() else {
            return Ok(());
        };
        if !desktop_file.exists() {
            return Ok(());
        }
        std::fs::remove_file(&desktop_file).map_err(|error| PlatformError::Io {
            path: desktop_file.to_string_lossy().to_string(),
            source: error,
        })
    }

    fn desktop_file_path() -> Option<PathBuf> {
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })?;
        Some(config_home.join("autostart/snapin.desktop"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_desktop_content() {
            assert!(DESKTOP_ENTRY_CONTENT.contains("Exec=snapin daemon"));
            assert!(DESKTOP_ENTRY_CONTENT.contains("X-GNOME-Autostart-enabled=true"));
        }

        #[test]
        fn test_desktop_file_path_uses_xdg_config_home() {
            let path = desktop_file_path().expect("测试环境必须提供 HOME");
            assert!(path.ends_with("autostart/snapin.desktop"));
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use crate::error::PlatformError;
    use crate::output_with_timeout;
    use std::process::Command;

    const REG_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const APP_NAME: &str = "Snapin";

    pub fn sync_autostart_windows(enabled: bool) -> Result<(), PlatformError> {
        if enabled {
            let current_exe = std::env::current_exe().map_err(|e| PlatformError::Io {
                path: "current_exe".to_string(),
                source: e,
            })?;
            let cmd_val = format!("\"{}\" daemon", current_exe.to_string_lossy());

            let mut command = Command::new("reg");
            command.args([
                "add", REG_KEY, "/v", APP_NAME, "/t", "REG_SZ", "/d", &cmd_val, "/f",
            ]);
            let status = output_with_timeout(command, super::AUTOSTART_COMMAND_TIMEOUT)
                .map_err(|e| PlatformError::Io {
                    path: REG_KEY.to_string(),
                    source: e,
                })?
                .status;

            if !status.success() {
                return Err(PlatformError::Autostart {
                    platform: "Windows".to_string(),
                    message: format!("写入 HKCU Run 注册表启动项失败，状态: {status}"),
                });
            }
            tracing::info!("已在 Windows 注册表中配置自启动项: {APP_NAME}");
        } else {
            let mut command = Command::new("reg");
            command.args(["delete", REG_KEY, "/v", APP_NAME, "/f"]);
            match output_with_timeout(command, super::AUTOSTART_COMMAND_TIMEOUT) {
                Ok(output) if output.status.success() => {
                    tracing::info!("已从 Windows 注册表中清理自启动项: {APP_NAME}");
                }
                Ok(output) => tracing::warn!(
                    "清理 Windows 注册表启动项返回非零状态，状态: {}，原因: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
                Err(error) => tracing::warn!("执行 Windows 注册表启动项清理失败: {error}"),
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use crate::error::PlatformError;
    use std::path::{Path, PathBuf};

    const PLIST_LABEL: &str = "local.wangle.snapin";

    pub fn sync_autostart_macos(enabled: bool) -> Result<(), PlatformError> {
        let Some(plist_path) = launch_agent_path() else {
            return Ok(());
        };
        if enabled {
            write_launch_agent(&plist_path)
        } else {
            remove_launch_agent(&plist_path);
            Ok(())
        }
    }

    /// 生成并原子替换 LaunchAgent 清单。
    fn write_launch_agent(plist_path: &Path) -> Result<(), PlatformError> {
        let content = launch_agent_plist_content()?;
        if let Some(parent) = plist_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| PlatformError::Io {
                path: parent.to_string_lossy().to_string(),
                source: error,
            })?;
        }
        write_launch_agent_atomically(plist_path, content.as_bytes())?;
        tracing::info!("已生成 macOS LaunchAgent 自启动清单: {:?}", plist_path);
        Ok(())
    }

    /// 构造引用当前可执行文件的 LaunchAgent plist。
    fn launch_agent_plist_content() -> Result<String, PlatformError> {
        let current_exe = std::env::current_exe().map_err(|error| PlatformError::Io {
            path: "current_exe".to_string(),
            source: error,
        })?;
        let executable = super::xml_escape(&current_exe.to_string_lossy());
        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{PLIST_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{executable}</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
        ))
    }

    /// 在目标目录创建临时文件并原子替换正式清单。
    fn write_launch_agent_atomically(
        plist_path: &Path,
        content: &[u8],
    ) -> Result<(), PlatformError> {
        let parent = plist_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let temp_path = parent.join(format!(
            ".{}.{}.tmp",
            PLIST_LABEL,
            snapin_foundation::AppPaths::generate_unique_id()
        ));
        if let Err(source) = snapin_foundation::AppPaths::write_secure_file(&temp_path, content) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(PlatformError::Io {
                path: temp_path.to_string_lossy().to_string(),
                source,
            });
        }
        if let Err(source) =
            snapin_foundation::AppPaths::replace_file_atomically(&temp_path, plist_path)
        {
            let _ = std::fs::remove_file(&temp_path);
            return Err(PlatformError::Io {
                path: plist_path.to_string_lossy().to_string(),
                source,
            });
        }
        Ok(())
    }

    /// 移除已有 LaunchAgent 清单。
    fn remove_launch_agent(plist_path: &Path) {
        if !plist_path.exists() {
            return;
        }
        if let Err(error) = std::fs::remove_file(plist_path) {
            tracing::warn!(
                "移除 macOS LaunchAgent 自启动清单失败，路径: {:?}，原因: {error}",
                plist_path
            );
            return;
        }
        tracing::info!("已移除 macOS LaunchAgent 自启动清单: {:?}", plist_path);
    }

    fn launch_agent_path() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(|home| {
            PathBuf::from(home).join(format!("Library/LaunchAgents/{PLIST_LABEL}.plist"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::xml_escape;

    #[test]
    fn test_xml_escape_protects_launch_agent_path() {
        assert_eq!(xml_escape("a&b<c>\"d'"), "a&amp;b&lt;c&gt;&quot;d&apos;");
    }
}
