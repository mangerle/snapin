//! 有界同步系统命令执行器。
//!
//! 设置中心、自启动同步和 GNOME 扩展部署需要在阻塞线程中读取命令输出；
//! `Command::output` 本身没有超时，对端卡死时会长期占用线程。本模块通过
//! 轮询子进程状态并强制终止超时进程，保证同步调用也具备明确上限。

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

/// 命令状态轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// 运行命令并返回输出，超过 `timeout` 时终止子进程并返回超时错误。
///
/// 该实现将标准输出和标准错误接入管道，适合 `gsettings`、`systemctl`、`reg`
/// 等预期输出较小的系统工具。调用方不得用于可能产生超大输出的命令，否则子进程
/// 可能在退出前被管道缓冲区阻塞。
///
/// # Errors
///
/// 命令无法启动、状态查询失败、输出读取失败或执行超时时返回 I/O 错误。
pub fn output_with_timeout(mut command: Command, timeout: Duration) -> std::io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let command_name = command.get_program().to_string_lossy().to_string();
    let mut child = command.spawn()?;
    let started = Instant::now();

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "系统命令执行超时，命令: {command_name}，上限: {}ms",
                        timeout.as_millis()
                    ),
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    };

    collect_output(child, status)
}

fn collect_output(mut child: Child, status: ExitStatus) -> std::io::Result<Output> {
    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_end(&mut stdout)?;
    }
    let mut stderr = Vec::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_end(&mut stderr)?;
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_with_timeout_collects_output() {
        #[cfg(unix)]
        let command = {
            let mut command = Command::new("sh");
            command.args(["-c", "printf hello"]);
            command
        };
        #[cfg(windows)]
        let command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "echo hello"]);
            command
        };

        let output =
            output_with_timeout(command, Duration::from_secs(1)).expect("无害命令应成功执行");
        assert!(output.status.success());
        assert!(!output.stdout.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_output_with_timeout_kills_hung_command() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 2"]);

        let error = output_with_timeout(command, Duration::from_millis(20))
            .expect_err("超时命令必须被终止");

        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    }
}
