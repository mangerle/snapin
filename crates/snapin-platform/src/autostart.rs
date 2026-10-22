//! Linux 开机自动启动管理（systemd 用户服务优先，XDG autostart 兜底）。
//!
//! deb 安装包通过 systemd 用户服务托管守护进程；开发环境或非 deb 安装则可能只有
//! XDG autostart。两种机制同时存在时，登录阶段会重复拉起守护进程。因此这里优先
//! 探测 systemd 单元：存在时只操作 systemd 并清理旧 XDG 文件，不存在时才写桌面项。

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

/// systemd 用户服务单元名
const SYSTEMD_SERVICE: &str = "snapin";

/// 同步开机自启动状态。
///
/// 优先操作已安装的 systemd 用户服务，并删除可能残留的 XDG autostart 文件，
/// 避免登录时被两条机制重复拉起；若系统没有可用的 systemd 单元，则回退到
/// XDG autostart 桌面项。
///
/// # Errors
///
/// systemd 命令执行失败、非零退出或桌面项文件读写失败时返回对应平台错误。
pub fn sync_autostart(enabled: bool) -> Result<(), PlatformError> {
    if systemd_unit_loaded()? {
        sync_systemd_service(enabled)?;
        remove_desktop_file()?;
        return Ok(());
    }

    sync_desktop_file(enabled)
}

/// 探测当前用户是否存在名为 snapin 的 systemd 单元。
///
/// `systemctl show` 在单元不存在或无用户级 systemd 会话时可能返回非零退出码，
/// 这类情况统一视为“无 systemd 单元”，由 XDG autostart 兜底而不是让设置保存失败。
fn systemd_unit_loaded() -> Result<bool, PlatformError> {
    let output = Command::new("systemctl")
        .args([
            "--user",
            "show",
            "-p",
            "LoadState",
            "--value",
            SYSTEMD_SERVICE,
        ])
        .output()
        .map_err(|error| PlatformError::Systemd {
            action: "探测".to_string(),
            service: SYSTEMD_SERVICE.to_string(),
            message: error.to_string(),
        })?;

    if !output.status.success() {
        return Ok(false);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim() == "loaded")
}

/// 启用或停用 systemd 用户服务的开机自启动。
///
/// 这里只修改 enable/disable 状态，不强制启动或停止当前会话中的服务：
/// 用户关闭自启后，正在运行的守护进程应继续提供服务，直到下次登录或手动退出。
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

/// 生成或移除 XDG autostart 桌面项。
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

/// 删除残留的 XDG autostart 桌面项；文件不存在时视为成功。
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

/// 计算当前用户的 XDG autostart 桌面项路径。
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
