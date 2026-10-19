//! 系统级服务生命周期管理辅助模块（systemd 与 GNOME 扩展）。

use std::process::Command;

/// 停止 systemd 用户服务以防被系统重启守活
pub fn stop_systemd_service() {
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "snapin"])
        .output();
}

/// 禁用 GNOME 扩展以彻底收起顶栏托盘图标
pub fn disable_gnome_extension() {
    let _ = Command::new("gnome-extensions")
        .args(["disable", "snapin@wangle.local"])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_function_pointer() {
        let fn_ptr = stop_systemd_service;
        assert!(!std::ptr::addr_of!(fn_ptr).is_null());
    }
}
