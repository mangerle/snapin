//! Unix Domain Socket 客户端通信调用封装。

use super::codec::{read_framed_message, write_framed_message};
use super::protocol::{get_socket_path, DaemonRequest, DaemonResponse, IPC_TIMEOUT};
use crate::error::DaemonError;
use std::path::Path;
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::time::timeout;

/// 客户端角色：向正在运行的守护进程发送强类型请求，并等待接收双向确认响应。
///
/// 若守护进程未运行（Socket 不存在或无进程监听），返回 `Ok(None)`；
/// 若发送并接收到响应，返回 `Ok(Some(response))`；
/// 若通信超时或套接字损坏，返回相应的 `Err(DaemonError)`。
///
/// # Errors
///
/// 套接字连接、请求写入、响应读取、协议处理或超时失败时返回 [`DaemonError`]。
pub async fn send_request_to_daemon(
    request: &DaemonRequest,
) -> Result<Option<DaemonResponse>, DaemonError> {
    let socket_path = get_socket_path();
    send_request_to_path(&socket_path, request).await
}

#[cfg(windows)]
pub(crate) fn windows_pipe_name(socket_path: &Path) -> String {
    let stem = socket_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("snapin");
    format!(r"\\.\pipe\{stem}-ipc")
}

/// 向指定路径的套接字发送请求并等待响应（便于单元测试隔离）
#[cfg(unix)]
/// # Errors
///
/// 套接字连接、请求写入、响应读取、协议处理或超时失败时返回 [`DaemonError`]。
pub async fn send_request_to_path(
    socket_path: &Path,
    request: &DaemonRequest,
) -> Result<Option<DaemonResponse>, DaemonError> {
    if !socket_path.exists() {
        return Ok(None);
    }

    let connect_fut = UnixStream::connect(socket_path);
    let mut stream = match timeout(IPC_TIMEOUT, connect_fut).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            // 不在这里删除套接字：accept 队列暂时耗尽、进程被 SIGSTOP 或极端负载
            // 都可能返回 ConnectionRefused，删除会误伤仍然存活的守护进程。
            // 残留套接字由 IpcListener::bind 在持有单实例锁时统一判定和清理。
            tracing::debug!("连接 IPC 套接字被拒绝，按服务未就绪处理: {e}");
            return Ok(None);
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(Err(e)) => {
            return Err(DaemonError::Socket {
                socket_path: socket_path.to_string_lossy().to_string(),
                message: format!("连接守护进程失败: {e}"),
            })
        }
        Err(_) => {
            return Err(DaemonError::Socket {
                socket_path: socket_path.to_string_lossy().to_string(),
                message: "连接守护进程超时".to_string(),
            })
        }
    };

    let send_fut = async {
        write_framed_message(&mut stream, request).await?;
        read_framed_message::<_, DaemonResponse>(&mut stream).await
    };

    match timeout(IPC_TIMEOUT, send_fut).await {
        Ok(res) => res.map(Some),
        Err(_) => Err(DaemonError::Socket {
            socket_path: socket_path.to_string_lossy().to_string(),
            message: "等待守护进程响应超时".to_string(),
        }),
    }
}

/// 向指定命名管道发送请求并等待响应（Windows 平台实现）
#[cfg(windows)]
pub async fn send_request_to_path(
    socket_path: &Path,
    request: &DaemonRequest,
) -> Result<Option<DaemonResponse>, DaemonError> {
    use tokio::net::windows::named_pipe::ClientOptions;

    let pipe_name = windows_pipe_name(socket_path);
    let client = match ClientOptions::new().open(&pipe_name) {
        Ok(client) => client,
        Err(e) if e.raw_os_error() == Some(2) => {
            // ERROR_FILE_NOT_FOUND 表示无服务监听管道
            return Ok(None);
        }
        Err(e) => {
            return Err(DaemonError::Socket {
                socket_path: pipe_name,
                message: format!("连接守护进程命名管道失败: {e}"),
            });
        }
    };

    let mut stream = client;
    let send_fut = async {
        write_framed_message(&mut stream, request).await?;
        read_framed_message::<_, DaemonResponse>(&mut stream).await
    };

    match timeout(IPC_TIMEOUT, send_fut).await {
        Ok(res) => res.map(Some),
        Err(_) => Err(DaemonError::Socket {
            socket_path: pipe_name,
            message: "等待守护进程响应超时".to_string(),
        }),
    }
}

/// 兼容旧版调用的客户端辅助函数：发送指令并判断守护进程是否成功响应
///
/// 若守护进程成功处理并确认（Ok/Pong/Status），返回 `Ok(true)`；
/// 若守护进程未运行，返回 `Ok(false)`；
/// 若守护进程返回明确业务处理失败（Error），返回 `Err(DaemonError::Business)`。
///
/// # Errors
///
/// 套接字连接、请求写入、响应读取、协议处理或超时失败时返回 [`DaemonError`]。
pub async fn send_command_to_daemon(command: &DaemonRequest) -> Result<bool, DaemonError> {
    match send_request_to_daemon(command).await? {
        Some(DaemonResponse::Ok)
        | Some(DaemonResponse::Pong)
        | Some(DaemonResponse::Status { .. }) => Ok(true),
        Some(DaemonResponse::Error(failure)) => Err(DaemonError::Business {
            code: failure.code,
            message: failure.message,
        }),
        None => Ok(false),
    }
}

/// 守护进程探测结果：区分“确实没有实例”与“套接字被占用但无法按当前协议通信”。
///
/// 升级后常见旧版本守护进程仍占用套接字，此时按“未运行”处理会派生新实例：
/// 新实例要么删掉存活套接字抢占路径（两个守护进程同时注册托盘与热键），
/// 要么反复启动失败。调用方必须先依据本枚举区分这两种状态再决定是否派生。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonProbe {
    /// 没有进程监听套接字
    Absent,
    /// 存在可按当前协议通信的守护进程
    Online,
    /// 套接字可建立连接，但对端未按当前协议应答（旧版本或已无响应）
    Unresponsive {
        /// 底层失败原因，用于向用户提示具体现场
        reason: String,
    },
}

/// 探测常驻守护进程的在线状态。
///
/// 只发起一次 Ping 往返：连接被拒绝或套接字不存在判定为未运行；收到任何响应判定为在线；
/// 连接成功却读取失败或超时判定为协议不兼容/无响应，从而避免误判为“未运行”而重复派生。
pub async fn probe_daemon() -> DaemonProbe {
    match send_request_to_daemon(&DaemonRequest::Ping).await {
        Ok(Some(_)) => DaemonProbe::Online,
        Ok(None) => DaemonProbe::Absent,
        Err(error) => DaemonProbe::Unresponsive {
            reason: error.to_string(),
        },
    }
}

/// 尝试向常驻守护进程发送热重载配置指令（非阻塞投递）。
///
/// 优先复用调用方已有的 Tokio 运行时，没有上下文时使用进程级共享任务运行时；
/// 不再为一次 IPC 通知创建临时线程或临时运行时。
pub fn notify_daemon_reload_config() {
    let runtime = snapin_runtime::TaskRuntime::shared().cloned();
    let Some(runtime) = runtime else {
        tracing::warn!("后台任务运行时不可用，配置热重载通知未发送");
        return;
    };
    if let Err(error) = runtime.spawn(async {
        if let Err(error) = send_command_to_daemon(&DaemonRequest::ReloadConfig).await {
            tracing::debug!("发送配置热重载通知失败: {error}");
        }
    }) {
        tracing::warn!("提交配置热重载通知任务失败: {error}");
    }
}
