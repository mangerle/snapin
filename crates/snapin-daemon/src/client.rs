//! Unix Domain Socket 客户端通信调用封装。

use super::codec::{read_framed_message, write_framed_message};
use super::protocol::{get_socket_path, DaemonRequest, DaemonResponse, IPC_TIMEOUT};
use crate::error::DaemonError;
use std::path::Path;
use tokio::net::UnixStream;
use tokio::time::timeout;

/// 客户端角色：向正在运行的守护进程发送强类型请求，并等待接收双向确认响应。
///
/// 若守护进程未运行（Socket 不存在或无进程监听），返回 `Ok(None)`；
/// 若发送并接收到响应，返回 `Ok(Some(response))`；
/// 若通信超时或套接字损坏，返回相应的 `Err(DaemonError)`。
pub async fn send_request_to_daemon(
    request: &DaemonRequest,
) -> Result<Option<DaemonResponse>, DaemonError> {
    let socket_path = get_socket_path();
    send_request_to_path(&socket_path, request).await
}

/// 向指定路径的套接字发送请求并等待响应（便于单元测试隔离）
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
            // 套接字文件残留但无进程监听，安全清理孤儿套接字文件
            let _ = std::fs::remove_file(socket_path);
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

/// 兼容旧版调用的客户端辅助函数：发送指令并判断守护进程是否成功响应
///
/// 若守护进程成功处理并确认（Ok/Pong/Status），返回 `Ok(true)`；
/// 若守护进程未运行，返回 `Ok(false)`；
/// 若守护进程返回明确业务处理失败（Error），返回 `Err(DaemonError::Business)`。
pub async fn send_command_to_daemon(command: &DaemonRequest) -> Result<bool, DaemonError> {
    match send_request_to_daemon(command).await? {
        Some(DaemonResponse::Ok)
        | Some(DaemonResponse::Pong)
        | Some(DaemonResponse::Status { .. }) => Ok(true),
        Some(DaemonResponse::Error(msg)) => Err(DaemonError::Business(msg)),
        None => Ok(false),
    }
}
