//! 有界同步系统命令执行器。
//!
//! 设置中心、自启动同步和 GNOME 扩展部署需要在阻塞线程中读取命令输出；
//! `Command::output` 本身没有超时，对端卡死时会长期占用线程。本模块通过
//! 并发排空 stdout/stderr、轮询子进程状态并强制终止超时进程，保证同步调用
//! 同时具备时间上限与内存上限。

use std::io::{ErrorKind, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// 命令状态轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// 单路标准输出或标准错误允许保留的最大字节数。
const MAX_COMMAND_OUTPUT_BYTES: usize = 1024 * 1024;
/// 管道单次读取缓冲区大小。
const PIPE_READ_BUFFER_BYTES: usize = 8 * 1024;

/// 单路管道读取结果。
struct CapturedPipe {
    bytes: Vec<u8>,
    total_bytes: usize,
}

impl CapturedPipe {
    fn exceeded_limit(&self) -> bool {
        self.total_bytes > MAX_COMMAND_OUTPUT_BYTES
    }
}

/// 运行命令并返回有界输出，超过 `timeout` 时终止子进程并返回超时错误。
///
/// stdout 与 stderr 在子进程运行期间由独立线程并发排空。单路输出超过固定
/// 上限时仍继续排空管道以防子进程阻塞，但命令结束后会返回明确错误而不是保留
/// 无界数据。
///
/// # Errors
///
/// 命令无法启动、状态查询失败、输出读取失败、输出超过上限或执行超时时返回
/// 带中文上下文的 I/O 错误。
pub fn output_with_timeout(command: Command, timeout: Duration) -> std::io::Result<Output> {
    let process = spawn_captured_process(command)?;
    finish_captured_process(process, timeout)
}

/// 已启动并配置好双路输出读取线程的子进程。
struct CapturedProcess {
    child: Child,
    stdout_reader: JoinHandle<std::io::Result<CapturedPipe>>,
    stderr_reader: JoinHandle<std::io::Result<CapturedPipe>>,
    command_name: String,
}

/// 启动子进程并并发排空 stdout/stderr。
fn spawn_captured_process(mut command: Command) -> std::io::Result<CapturedProcess> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // 让子进程成为独立进程组组长，超时可连同其派生进程一起终止，
        // 避免孙进程继续持有 stdout/stderr 管道导致读取线程长时间阻塞。
        command.process_group(0);
    }
    let command_name = command.get_program().to_string_lossy().to_string();
    let mut child = command.spawn()?;
    let stdout_reader =
        match spawn_pipe_reader(child.stdout.take(), "标准输出", command_name.clone()) {
            Ok(reader) => reader,
            Err(error) => {
                terminate_child(&mut child);
                return Err(error);
            }
        };
    let stderr_reader =
        match spawn_pipe_reader(child.stderr.take(), "标准错误", command_name.clone()) {
            Ok(reader) => reader,
            Err(error) => {
                terminate_child(&mut child);
                let _ = join_pipe_reader(stdout_reader, "标准输出", &command_name);
                return Err(error);
            }
        };
    Ok(CapturedProcess {
        child,
        stdout_reader,
        stderr_reader,
        command_name,
    })
}

/// 等待子进程结束、收集双路输出并执行上限校验。
fn finish_captured_process(
    mut process: CapturedProcess,
    timeout: Duration,
) -> std::io::Result<Output> {
    let status = wait_for_status(&mut process.child, timeout, &process.command_name);
    let stdout = join_pipe_reader(process.stdout_reader, "标准输出", &process.command_name);
    let stderr = join_pipe_reader(process.stderr_reader, "标准错误", &process.command_name);
    let stdout = stdout?;
    let stderr = stderr?;
    let status = status?;

    if stdout.exceeded_limit() {
        return Err(output_limit_error(
            &process.command_name,
            "标准输出",
            stdout.total_bytes,
        ));
    }
    if stderr.exceeded_limit() {
        return Err(output_limit_error(
            &process.command_name,
            "标准错误",
            stderr.total_bytes,
        ));
    }
    Ok(Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

/// 为单路管道启动并发读取线程。
fn spawn_pipe_reader<R>(
    pipe: Option<R>,
    stream_name: &'static str,
    command_name: String,
) -> std::io::Result<JoinHandle<std::io::Result<CapturedPipe>>>
where
    R: Read + Send + 'static,
{
    let mut pipe = pipe.ok_or_else(|| {
        std::io::Error::new(
            ErrorKind::BrokenPipe,
            format!("系统命令[{command_name}]未创建{stream_name}管道"),
        )
    })?;
    thread::Builder::new()
        .name(format!("snapin-command-{stream_name}"))
        .spawn(move || read_pipe_limited(&mut pipe))
}

/// 读取单路管道并保留固定长度前缀，同时持续排空剩余数据。
fn read_pipe_limited(pipe: &mut impl Read) -> std::io::Result<CapturedPipe> {
    let mut bytes = Vec::with_capacity(PIPE_READ_BUFFER_BYTES.min(MAX_COMMAND_OUTPUT_BYTES));
    let mut buffer = [0u8; PIPE_READ_BUFFER_BYTES];
    let mut total_bytes = 0usize;

    loop {
        let read = pipe.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(read);
        let remaining = MAX_COMMAND_OUTPUT_BYTES.saturating_sub(bytes.len());
        if remaining > 0 {
            bytes.extend_from_slice(&buffer[..read.min(remaining)]);
        }
    }

    Ok(CapturedPipe { bytes, total_bytes })
}

/// 等待子进程结束；超时或状态查询失败时终止并回收子进程。
fn wait_for_status(
    child: &mut Child,
    timeout: Duration,
    command_name: &str,
) -> std::io::Result<ExitStatus> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                terminate_child(child);
                return Err(std::io::Error::new(
                    ErrorKind::TimedOut,
                    format!(
                        "系统命令执行超时，命令: {command_name}，上限: {}ms",
                        timeout.as_millis()
                    ),
                ));
            }
            Err(error) => {
                terminate_child(child);
                return Err(std::io::Error::new(
                    error.kind(),
                    format!("查询系统命令状态失败，命令: {command_name}，原因: {error}"),
                ));
            }
        }
    }
}

/// 终止并回收子进程，忽略重复终止产生的错误。
fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::Kill);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// 等待管道读取线程结束并映射线程崩溃错误。
fn join_pipe_reader(
    reader: JoinHandle<std::io::Result<CapturedPipe>>,
    stream_name: &str,
    command_name: &str,
) -> std::io::Result<CapturedPipe> {
    reader.join().map_err(|_| {
        std::io::Error::other(format!(
            "读取系统命令{stream_name}的线程异常退出，命令: {command_name}"
        ))
    })?
}

/// 构造输出超过固定上限的中文错误。
fn output_limit_error(
    command_name: &str,
    stream_name: &str,
    actual_bytes: usize,
) -> std::io::Error {
    std::io::Error::new(
        ErrorKind::InvalidData,
        format!(
            "系统命令{stream_name}超过上限，命令: {command_name}，实际: {actual_bytes} 字节，上限: {MAX_COMMAND_OUTPUT_BYTES} 字节"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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

    /// 大量但未超限的输出必须在子进程运行期间被并发排空，不能误触发超时。
    #[cfg(unix)]
    #[test]
    fn test_output_with_timeout_drains_large_pipe_concurrently() {
        let mut command = Command::new("sh");
        command.args(["-c", "head -c 262144 /dev/zero"]);
        let output =
            output_with_timeout(command, Duration::from_secs(2)).expect("并发排空管道后应成功结束");
        assert_eq!(output.stdout.len(), 262_144);
    }

    #[cfg(unix)]
    #[test]
    fn test_output_with_timeout_kills_hung_command() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 2"]);

        let error = output_with_timeout(command, Duration::from_millis(20))
            .expect_err("超时命令必须被终止");

        assert_eq!(error.kind(), ErrorKind::TimedOut);
    }

    /// 管道超过上限时只保留固定前缀，并报告实际读取的总字节数。
    #[test]
    fn test_read_pipe_limited_keeps_bounded_prefix() {
        let input = vec![7u8; MAX_COMMAND_OUTPUT_BYTES + 1024];
        let captured = read_pipe_limited(&mut Cursor::new(input)).expect("读取有限管道失败");
        assert_eq!(captured.bytes.len(), MAX_COMMAND_OUTPUT_BYTES);
        assert_eq!(captured.total_bytes, MAX_COMMAND_OUTPUT_BYTES + 1024);
        assert!(captured.exceeded_limit());
    }
}
