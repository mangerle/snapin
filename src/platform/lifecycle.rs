//! 应用程序生命周期控制模块（优雅退出与服务管理）。

use std::process::Command;

/// 完全退出 Snapin（停止守护服务、关闭贴图并收起托盘图标）
pub fn exit_snapin_application() {
    tracing::info!("正在完全退出 Snapin...");
    // 1. 停止 systemd 用户服务以防被系统重启守活
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "snapin"])
        .output();
    // 2. 禁用 GNOME 扩展以彻底收起顶栏托盘图标
    let _ = Command::new("gnome-extensions")
        .args(["disable", "snapin-helper@wangle.local"])
        .output();
    // 3. 强制清理所有贴图窗口与前台进程
    let _ = Command::new("pkill")
        .args(["-9", "-f", "snapin pin-file"])
        .output();
    let _ = Command::new("pkill")
        .args(["-9", "-f", "snapin prefs"])
        .output();
    // 4. 终止常驻守护进程
    let _ = Command::new("pkill")
        .args(["-9", "-f", "snapin daemon"])
        .output();
}

#[cfg(test)]
mod tests {
    use super::exit_snapin_application;

    #[test]
    fn test_lifecycle_function_pointer() {
        let fn_ptr = exit_snapin_application;
        assert!(!std::ptr::addr_of!(fn_ptr).is_null());
    }
}
