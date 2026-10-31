//! 跨平台开机自启动管理。
//!
//! - Linux: systemd 用户服务优先，XDG autostart 桌面项兜底；
//! - Windows: 注册表当前用户启动项 (HKCU\Software\Microsoft\Windows\CurrentVersion\Run)；
//! - macOS: 用户级 LaunchAgents 服务代理清单 (~/Library/LaunchAgents/local.wangle.snapin.plist)。

use crate::error::PlatformError;

/// 同步开机自启动配置状态。
///
/// 针对不同操作系统自动应用对应的原生自启持久化通路。
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
        let output = match Command::new("systemctl")
            .args([
                "--user",
                "show",
                "-p",
                "LoadState",
                "--value",
                SYSTEMD_SERVICE,
            ])
            .output()
        {
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
        let output = Command::new("systemctl")
            .args(["--user", action, SYSTEMD_SERVICE])
            .output()
            .map_err(|error| PlatformError::Systemd {
                action: action.to_string(),
                service: SYSTEMD_SERVICE.to_string(),
                message: error.to_string(),
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
            std::fs::write(&desktop_file, DESKTOP_ENTRY_CONTENT).map_err(|error| {
                PlatformError::Io {
                    path: desktop_file.to_string_lossy().to_string(),
                    source: error,
                }
            })?;
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
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".config/autostart/snapin.desktop"))
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
        fn test_desktop_file_path_uses_home() {
            let path = desktop_file_path().expect("测试环境必须提供 HOME");
            assert!(path.ends_with(".config/autostart/snapin.desktop"));
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use crate::error::PlatformError;
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

            let status = Command::new("reg")
                .args([
                    "add", REG_KEY, "/v", APP_NAME, "/t", "REG_SZ", "/d", &cmd_val, "/f",
                ])
                .status()
                .map_err(|e| PlatformError::Io {
                    path: REG_KEY.to_string(),
                    source: e,
                })?;

            if !status.success() {
                return Err(PlatformError::Capture {
                    message: format!("写入 Windows 注册表启动项失败，返回码: {status}"),
                });
            }
            tracing::info!("已在 Windows 注册表中配置自启动项: {APP_NAME}");
        } else {
            let _ = Command::new("reg")
                .args(["delete", REG_KEY, "/v", APP_NAME, "/f"])
                .status();
            tracing::info!("已从 Windows 注册表中清理自启动项: {APP_NAME}");
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use crate::error::PlatformError;
    use std::path::PathBuf;

    const PLIST_LABEL: &str = "local.wangle.snapin";

    pub fn sync_autostart_macos(enabled: bool) -> Result<(), PlatformError> {
        let Some(plist_path) = launch_agent_path() else {
            return Ok(());
        };

        if enabled {
            let current_exe = std::env::current_exe().map_err(|e| PlatformError::Io {
                path: "current_exe".to_string(),
                source: e,
            })?;
            let exe_str = current_exe.to_string_lossy();

            let plist_content = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{PLIST_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe_str}</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
            );

            if let Some(parent) = plist_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| PlatformError::Io {
                    path: parent.to_string_lossy().to_string(),
                    source: e,
                })?;
            }

            std::fs::write(&plist_path, plist_content).map_err(|e| PlatformError::Io {
                path: plist_path.to_string_lossy().to_string(),
                source: e,
            })?;
            tracing::info!("已生成 macOS LaunchAgent 自启动清单: {:?}", plist_path);
        } else if plist_path.exists() {
            let _ = std::fs::remove_file(&plist_path);
            tracing::info!("已移除 macOS LaunchAgent 自启动清单: {:?}", plist_path);
        }

        Ok(())
    }

    fn launch_agent_path() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(|home| {
            PathBuf::from(home).join(format!("Library/LaunchAgents/{PLIST_LABEL}.plist"))
        })
    }
}
