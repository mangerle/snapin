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
    mut stream: S,
    sender: Sender<IpcInvocation>,
    connections: &mut JoinSet<()>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    // 达到上限时先等待一个旧连接结束，形成真实背压，绝不能继续无界 spawn。
    if connections.len() >= MAX_CONCURRENT_CONNECTIONS {
        if let Some(Err(error)) = connections.join_next().await {
            tracing::warn!("IPC 连接任务异常退出: {error}");
        }
    }
    connections.spawn(async move {
        let read_res = timeout(
            IPC_TIMEOUT,
            read_framed_message::<_, DaemonRequest>(&mut stream),
        )
        .await;

        match read_res {
            Ok(Ok(request)) => {
                let (resp_tx, resp_rx) = oneshot::channel();
                let cancelled = CancellationToken::new();
                let invocation = IpcInvocation {
                    request,
                    responder: resp_tx,
                    cancelled: cancelled.clone(),
                };
                if sender.send(invocation).await.is_ok() {
                    let response = match timeout(IPC_TIMEOUT, resp_rx).await {
                        Ok(Ok(resp)) => resp,
                        Ok(Err(_)) => {
                            DaemonResponse::error(DaemonErrorCode::Unavailable, "业务执行器已关闭")
                        }
                        Err(_) => {
                            // 客户端已经超时：通知业务执行方在下一个阶段边界停止，
                            // 避免“客户端收到失败、服务端仍完成副作用”的幽灵执行。
                            cancelled.cancel();
                            DaemonResponse::error(DaemonErrorCode::Timeout, "业务处理超时")
                        }
                    };
                    let _ =
                        timeout(IPC_TIMEOUT, write_framed_message(&mut stream, &response)).await;
                } else {
                    let _ = timeout(
                        IPC_TIMEOUT,
                        write_framed_message(
                            &mut stream,
                            &DaemonResponse::error(
                                DaemonErrorCode::Unavailable,
                                "守护进程主循环已关闭",
                            ),
                        ),
                    )
                    .await;
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("接收客户端 IPC 请求异常: {e}");
            }
            Err(_) => {
                tracing::warn!("接收客户端 IPC 请求超时，主动断开连接");
            }
        }
    });
}
