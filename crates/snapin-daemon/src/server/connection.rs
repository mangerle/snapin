//! IPC 单连接生命周期：请求帧读取、业务回执等待与响应回写。
//!
//! 连接处理器只负责协议边界与每请求超时，不执行业务命令；业务分发由应用层
//! 的 IPC 处理器完成，监听循环只负责 accept 与背压。

use super::MAX_CONCURRENT_CONNECTIONS;
use crate::codec::{read_framed_message, write_framed_message};
use crate::protocol::{DaemonErrorCode, DaemonRequest, DaemonResponse, IpcInvocation, IPC_TIMEOUT};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// 为单个客户端连接生成异步处理任务，完成请求帧反序列化、业务调用与响应帧回传
pub(super) async fn spawn_connection_handler<S>(
    stream: S,
    sender: Sender<IpcInvocation>,
    connections: &mut JoinSet<()>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    wait_for_capacity(connections).await;
    connections.spawn(handle_connection(stream, sender));
}

/// 达到并发上限时等待一个旧连接结束，形成真实背压而不是继续无界派生。
async fn wait_for_capacity(connections: &mut JoinSet<()>) {
    if connections.len() < MAX_CONCURRENT_CONNECTIONS {
        return;
    }
    if let Some(Err(error)) = connections.join_next().await {
        tracing::warn!("IPC 连接任务异常退出: {error}");
    }
}

/// 处理单条连接：读取一个请求、等待业务回执并写回响应。
async fn handle_connection<S>(mut stream: S, sender: Sender<IpcInvocation>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    let Some(request) = read_request(&mut stream).await else {
        return;
    };
    let response = execute_request(&sender, request).await;
    write_response(&mut stream, &response).await;
}

/// 在固定超时内读取并反序列化一条请求帧。
async fn read_request<S>(stream: &mut S) -> Option<DaemonRequest>
where
    S: tokio::io::AsyncRead + Unpin,
{
    match timeout(IPC_TIMEOUT, read_framed_message(stream)).await {
        Ok(Ok(request)) => Some(request),
        Ok(Err(error)) => {
            tracing::warn!("接收客户端 IPC 请求异常: {error}");
            None
        }
        Err(_) => {
            tracing::warn!("接收客户端 IPC 请求超时，主动断开连接");
            None
        }
    }
}

/// 将请求交给业务主循环，并在超时或执行器关闭时生成确定响应。
async fn execute_request(sender: &Sender<IpcInvocation>, request: DaemonRequest) -> DaemonResponse {
    let (response_tx, response_rx) = oneshot::channel();
    let cancelled = CancellationToken::new();
    let invocation = IpcInvocation {
        request,
        responder: response_tx,
        cancelled: cancelled.clone(),
    };
    if sender.send(invocation).await.is_err() {
        return DaemonResponse::error(DaemonErrorCode::Unavailable, "守护进程主循环已关闭");
    }
    match timeout(IPC_TIMEOUT, response_rx).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => DaemonResponse::error(DaemonErrorCode::Unavailable, "业务执行器已关闭"),
        Err(_) => {
            // 客户端已经超时：通知业务执行方在下一个阶段边界停止，
            // 避免“客户端收到失败、服务端仍完成副作用”的幽灵执行。
            cancelled.cancel();
            DaemonResponse::error(DaemonErrorCode::Timeout, "业务处理超时")
        }
    }
}

/// 在固定超时内写回响应帧；连接已关闭时忽略错误。
async fn write_response<S>(stream: &mut S, response: &DaemonResponse)
where
    S: tokio::io::AsyncWrite + Unpin,
{
    let _ = timeout(IPC_TIMEOUT, write_framed_message(stream, response)).await;
}
