//! 系统级服务生命周期管理与子进程安全收割模块（systemd 与 GNOME 扩展）。
//!
//! 所有后台系统命令统一提交到进程级 [`TaskRuntime`]，由 Tokio 进程驱动完成
//! `.wait().await` 收割；不再为每条命令分支创建临时 reaper 线程或裸 `tokio::spawn`，
//! 从而把子进程取消、退出等待与 panic 观测纳入同一套后台任务契约。

use crate::error::PlatformError;
use snapin_runtime::{TaskPool, TaskRuntime};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

/// 单个外部系统命令的最长等待时间，超时后强制终止，防止挂死进程永久占用任务名额。
const SYSTEM_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// systemd 服务控制命令的最长等待时间。
const SYSTEMD_COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
/// systemd 用户服务单元名称。
const SYSTEMD_SERVICE: &str = "snapin";

/// 停用 systemd 用户服务以防系统重启守活。
///
/// 该调用位于异步退出路径，必须使用 tokio 进程并设置超时；超时后强杀
/// `systemctl` 本身，避免退出流程被系统总线卡住。
pub async fn stop_systemd_service() {
    #[cfg(target_os = "linux")]
    {
        let spawned = tokio::process::Command::new("systemctl")
            .args(["--user", "stop", "snapin"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        match spawned {
            Ok(mut child) => {
                match tokio::time::timeout(Duration::from_secs(15), child.wait()).await {
                    Ok(Ok(status)) if status.success() => {}
                    Ok(Ok(status)) => {
                        tracing::warn!("停止 systemd 用户服务返回非零状态: {status}");
                    }
                    Ok(Err(error)) => tracing::warn!("等待 systemd 停止命令失败: {error}"),
                    Err(_) => {
                        tracing::warn!("等待 systemd 停止命令超时，强制终止");
                        let _ = child.kill().await;
                    }
                }
            }
            Err(error) => tracing::warn!("无法执行 systemctl --user stop snapin: {error}"),
        }
    }
}

/// 启动升级后的常驻进程。
///
/// Linux 在 systemd 用户单元已加载时交由 systemd 托管并完成子进程回收；未安装或
/// 无法探测到该单元时直接派生独立进程，兼容 XDG autostart 与手工运行环境。
/// Windows/macOS 没有对应的用户服务，同样直接派生独立进程；父设置进程退出后
/// 交由操作系统接管长期运行的服务进程。
///
/// # Errors
///
/// systemd 服务启动失败或超时，或独立进程无法派生时返回中文错误。
pub async fn start_daemon_process(executable: &Path) -> Result<(), PlatformError> {
    #[cfg(target_os = "linux")]
    {
        if systemd_user_service_loaded().await {
            return start_systemd_service().await;
        }
    }

    spawn_daemon_process(executable)
}

/// 检查 systemd 用户单元是否已安装并加载，用于选择托管启动或独立派生。
#[cfg(target_os = "linux")]
async fn systemd_user_service_loaded() -> bool {
    let output = tokio::process::Command::new("systemctl")
        .args([
            "--user",
            "show",
            "-p",
            "LoadState",
            "--value",
            SYSTEMD_SERVICE,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output();

    match tokio::time::timeout(SYSTEMD_COMMAND_TIMEOUT, output).await {
        Ok(Ok(output)) => {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "loaded"
        }
        Ok(Err(error)) => {
            tracing::debug!("探测 systemd 用户服务失败，将直接派生守护进程: {error}");
            false
        }
        Err(_) => {
            tracing::debug!("探测 systemd 用户服务超时，将直接派生守护进程");
            false
        }
    }
}

/// 通过 systemd 用户服务启动守护进程，由服务管理器承担长期生命周期管理。
#[cfg(target_os = "linux")]
async fn start_systemd_service() -> Result<(), PlatformError> {
    let spawned = tokio::process::Command::new("systemctl")
        .args(["--user", "start", SYSTEMD_SERVICE])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(SYSTEMD_COMMAND_TIMEOUT, spawned).await {
        Ok(Ok(output)) if output.status.success() => Ok(()),
        Ok(Ok(output)) => Err(PlatformError::Systemd {
            action: "启动".to_string(),
            service: SYSTEMD_SERVICE.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }),
        Ok(Err(error)) => Err(PlatformError::Systemd {
            action: "启动".to_string(),
            service: SYSTEMD_SERVICE.to_string(),
            message: format!("执行 systemctl --user start snapin 失败: {error}"),
        }),
        Err(_) => Err(PlatformError::Systemd {
            action: "启动".to_string(),
            service: SYSTEMD_SERVICE.to_string(),
            message: "等待 systemctl --user start snapin 超时".to_string(),
        }),
    }
}

/// 直接派生独立守护进程；调用方必须在随后退出，交由系统接管子进程生命周期。
fn spawn_daemon_process(executable: &Path) -> Result<(), PlatformError> {
    std::process::Command::new(executable)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| PlatformError::ProcessStart {
            executable: executable.display().to_string(),
            message: error.to_string(),
        })
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
    let Some(runtime) = TaskRuntime::shared() else {
        tracing::error!("后台进程运行时不可用，已放弃启动系统命令: {program}");
        return;
    };

    let program = program.to_string();
    let arguments: Vec<OsString> = args.iter().map(|arg| arg.as_ref().to_os_string()).collect();
    let command_name = program.clone();
    if let Err(error) = runtime.spawn_in(TaskPool::Process, async move {
        let spawned = tokio::process::Command::new(&program)
            .args(&arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        match spawned {
            Ok(mut child) => match tokio::time::timeout(SYSTEM_COMMAND_TIMEOUT, child.wait()).await
            {
                Ok(Ok(status)) => {
                    if !status.success() {
                        tracing::warn!("系统子进程退出码非零 [{program}]，状态: {status}");
                    }
                }
                Ok(Err(error)) => {
                    tracing::warn!("等待系统子进程退出失败 [{program}]: {error}");
                }
                Err(_) => {
                    tracing::warn!(
                        "系统子进程等待超时（{} 秒），强制终止 [{program}]",
                        SYSTEM_COMMAND_TIMEOUT.as_secs()
                    );
                    if let Err(error) = child.kill().await {
                        tracing::warn!("强制终止系统子进程失败 [{program}]: {error}");
                    }
                }
            },
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

/// 使用系统默认浏览器打开经过协议白名单校验的网页链接。
///
/// # Errors
///
/// 链接为空、超过 4096 字节、包含控制字符，或协议不是 `http`/`https` 时返回
/// [`crate::error::PlatformError::Unsupported`]；命令派生失败只记录日志。
pub fn open_url(url: &str) -> Result<(), crate::error::PlatformError> {
    let url = url.trim();
    let valid_scheme = url.starts_with("https://") || url.starts_with("http://");
    let valid_text = url.len() <= 4096 && !url.chars().any(char::is_control);
    if !valid_scheme || !valid_text {
        return Err(crate::error::PlatformError::Unsupported {
            capability: format!("仅允许打开 http/https 安全网页链接: {url}"),
        });
    }

    #[cfg(target_os = "windows")]
    {
        spawn_detached_system_command("explorer", &[url]);
    }
    #[cfg(target_os = "macos")]
    {
        spawn_detached_system_command("open", &[url]);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        spawn_detached_system_command("xdg-open", &[url]);
    }
    Ok(())
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

    /// 网页打开入口必须拒绝本地文件和自定义协议，避免二维码内容逃逸到系统处理器。
    #[test]
    fn test_open_url_rejects_unsafe_schemes() {
        for unsafe_url in ["file:///etc/passwd", "javascript:alert(1)", "snapin://run"] {
            assert!(
                open_url(unsafe_url).is_err(),
                "危险链接不得放行: {unsafe_url}"
            );
        }
    }
}
