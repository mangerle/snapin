//! Linux XDG 开机自动启动桌面项管理。

use std::path::PathBuf;

const DESKTOP_ENTRY_CONTENT: &str = r#"[Desktop Entry]
Type=Application
Name=Snapin
Comment=Snapin 截图与贴图守护服务
Exec=snapin daemon
Terminal=false
Categories=Utility;
X-GNOME-Autostart-enabled=true
"#;

/// 同步开机启动状态：true 则生成 autostart 项，false 则安全移除
pub fn sync_autostart_desktop_file(enabled: bool) {
    if let Ok(home) = std::env::var("HOME") {
        let autostart_dir = PathBuf::from(home).join(".config/autostart");
        let desktop_file = autostart_dir.join("snapin.desktop");

        if enabled {
            let _ = std::fs::create_dir_all(&autostart_dir);
            let _ = std::fs::write(&desktop_file, DESKTOP_ENTRY_CONTENT);
        } else if desktop_file.exists() {
            let _ = std::fs::remove_file(&desktop_file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_content() {
        assert!(DESKTOP_ENTRY_CONTENT.contains("Exec=snapin daemon"));
    }
}
