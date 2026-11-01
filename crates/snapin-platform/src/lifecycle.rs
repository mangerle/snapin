//! 系统级服务生命周期管理与子进程安全收割模块（systemd 与 GNOME 扩展）。
//!
//! 所有后台系统命令统一提交到进程级 [`TaskRuntime`]，由 Tokio 进程驱动完成
//! `.wait().await` 收割；不再为每条命令分支创建临时 reaper 线程或裸 `tokio::spawn`，
//! 从而把子进程取消、退出等待与 panic 观测纳入同一套后台任务契约。

use snapin_runtime::TaskRuntime;
use std::ffi::{OsStr, OsString};
use std::process::Stdio;

/// 停用 systemd 用户服务以防系统重启守活。
pub fn stop_systemd_service() {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", "snapin"])
            .output();
    }
}

/// 启用配套 GNOME Shell 扩展并自动收割子进程状态，杜绝僵尸进程
pub fn enable_gnome_extension() {
    #[cfg(target_os = "linux")]
    spawn_detached_system_command("gnome-extensions", &["enable", "snapin@wangle.local"]);
}

/// 在后台异步启动并收割外部系统子进程，防止在操作系统中残留僵尸进程 (Zombie PCB)。
///
/// 命令进入进程级后台运行时队列；队列满或运行时不可用时记录警告并放弃本次派生，
/// 不阻塞调用线程，也不会创建不受追踪的临时线程。
pub fn spawn_detached_system_command<P: AsRef<OsStr>>(program: &str, args: &[P]) {
    let Some(runtime) = TaskRuntime::shared("snapin-platform-process", 16) else {
        tracing::error!("后台进程运行时不可用，已放弃启动系统命令: {program}");
        return;
    };

    let program = program.to_string();
    let arguments: Vec<OsString> = args.iter().map(|arg| arg.as_ref().to_os_string()).collect();
    let command_name = program.clone();
    if let Err(error) = runtime.spawn(async move {
        let spawned = tokio::process::Command::new(&program)
            .args(&arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Ok(mut child) => {
                if let Err(error) = child.wait().await {
                    tracing::warn!("等待系统子进程退出失败 [{program}]: {error}");
                }
            }
            Err(error) => tracing::warn!("派生后台系统子进程失败 [{program}]: {error}"),
        }
    }) {
        tracing::warn!("提交系统子进程任务失败 [{command_name}]: {error}");
    }
}

/// 在系统文件管理器中打开指定目录或文件并自动收割子进程，杜绝僵尸进程
pub fn open_path_in_file_manager<P: AsRef<std::path::Path>>(path: P) {
    let p = path.as_ref().to_path_buf();
    #[cfg(target_os = "windows")]
    {
        spawn_detached_system_command("explorer", &[p.as_os_str()]);
    }
    #[cfg(target_os = "macos")]
    {
        spawn_detached_system_command("open", &[p.as_os_str()]);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        spawn_detached_system_command("xdg-open", &[p.as_os_str()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_detached_system_command_executes_safely() {
        // 测试拉起标准无害命令并验证其能在统一运行时中正常回收，不触发 panic
        spawn_detached_system_command("true", &[""]);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
