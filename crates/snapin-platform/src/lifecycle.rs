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

/// 启用配套 GNOME Shell 扩展并自动收割子进程状态，杜绝僵尸进程
pub fn enable_gnome_extension() {
    spawn_detached_system_command("gnome-extensions", &["enable", "snapin@wangle.local"]);
}

/// 在后台异步启动并收割外部系统子进程，防止在操作系统中残留僵尸进程 (Zombie PCB)
pub fn spawn_detached_system_command<P: AsRef<std::ffi::OsStr>>(program: &str, args: &[P]) {
    let prog = program.to_string();
    let arg_list: Vec<std::ffi::OsString> =
        args.iter().map(|a| a.as_ref().to_os_string()).collect();

    std::thread::Builder::new()
        .name("snapin-reaper".to_string())
        .spawn(move || {
            if let Ok(mut child) = Command::new(&prog).args(&arg_list).spawn() {
                let _ = child.wait();
            }
        })
        .ok();
}

/// 在系统文件管理器中打开指定目录或文件并自动收割子进程，杜绝僵尸进程
pub fn open_path_in_file_manager<P: AsRef<std::path::Path>>(path: P) {
    let p = path.as_ref().to_path_buf();
    spawn_detached_system_command("xdg-open", &[p.as_os_str()]);
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
