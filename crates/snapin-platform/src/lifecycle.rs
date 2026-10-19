//! 系统级服务生命周期管理与子进程安全收割模块（systemd 与 GNOME 扩展）。
//!
//! 提供基于单一常驻收割线程与无继承 Stdio 的子进程派生机制，消除系统线程滥用并杜绝僵尸进程。

use std::ffi::{OsStr, OsString};
use std::process::{Command, Stdio};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;

static COMMAND_DISPATCHER: OnceLock<SyncSender<(String, Vec<OsString>)>> = OnceLock::new();

fn get_command_dispatcher() -> &'static SyncSender<(String, Vec<OsString>)> {
    COMMAND_DISPATCHER.get_or_init(|| {
        let (tx, rx) = sync_channel::<(String, Vec<OsString>)>(32);
        std::thread::Builder::new()
            .name("snapin-cmd-reaper".to_string())
            .spawn(move || {
                while let Ok((prog, args)) = rx.recv() {
                    if let Ok(mut child) = Command::new(&prog)
                        .args(&args)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                    {
                        let _ = child.wait();
                    }
                }
            })
            .expect("初始化系统命令收割线程失败");
        tx
    })
}

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
pub fn spawn_detached_system_command<P: AsRef<OsStr>>(program: &str, args: &[P]) {
    let prog = program.to_string();
    let arg_list: Vec<OsString> = args.iter().map(|a| a.as_ref().to_os_string()).collect();

    // 优先尝试将任务分发给全局常驻单例收割器线程，避免无节制为每次命令创建系统原生线程栈
    if let Err(e) = get_command_dispatcher().try_send((prog.clone(), arg_list.clone())) {
        tracing::debug!("命令收割器通道繁忙或降级，采用就地异步子进程执行: {e}");
        if let Ok(mut child) = Command::new(&prog)
            .args(&arg_list)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let _ = child.wait();
        }
    }
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
    fn test_spawn_detached_system_command_executes_safely() {
        // 测试拉起标准无害命令并验证其能在后台正常回收，不触发 panic
        spawn_detached_system_command("true", &[""]);
        // 短暂休眠确保后台收割线程完成 wait 调用
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
