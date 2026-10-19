//! Unix Domain Socket 服务端监听器实现。

use super::client::send_request_to_path;
use super::codec::{read_framed_message, write_framed_message};
use super::protocol::{get_socket_path, DaemonRequest, DaemonResponse, IpcInvocation, IPC_TIMEOUT};
use crate::error::DaemonError;
use std::path::PathBuf;
use tokio::net::UnixListener;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::time::timeout;

/// 服务端角色：绑定并监听 Unix Domain Socket，将收到的命令分发给事件主循环
pub struct IpcListener {
    socket_path: PathBuf,
    listener: UnixListener,
}

impl IpcListener {
    /// 绑定套接字并初始化监听器，若检测到已有存活实例则拒绝重复启动，若为残留文件则自动清理
    pub async fn bind(custom_path: Option<PathBuf>) -> Result<Self, DaemonError> {
        let socket_path = custom_path.unwrap_or_else(get_socket_path);

        if socket_path.exists() {
            // 发送 Ping 试探已有实例是否真实存活
            if let Ok(Some(DaemonResponse::Pong)) =
                send_request_to_path(&socket_path, &DaemonRequest::Ping).await
            {
                return Err(DaemonError::Socket {
                    socket_path: socket_path.to_string_lossy().to_string(),
                    message: "已有另一个 Snapin 守护进程在运行中".to_string(),
                });
            }
            let _ = std::fs::remove_file(&socket_path);
        }

        let listener = UnixListener::bind(&socket_path).map_err(|e| DaemonError::Socket {
            socket_path: socket_path.to_string_lossy().to_string(),
            message: format!("绑定 IPC 套接字失败: {e}"),
        })?;

        tracing::info!("Snapin IPC 监听服务就绪，套接字路径: {:?}", socket_path);
        Ok(Self {
            socket_path,
            listener,
        })
    }

    /// 启动常驻异步监听循环，将解码出的请求与响应回执派发给有界通道
    pub async fn run_loop(self, cmd_sender: Sender<IpcInvocation>) {
        loop {
            match self.listener.accept().await {
                Ok((mut stream, _addr)) => {
                    let sender = cmd_sender.clone();
                    tokio::spawn(async move {
                        let read_res = timeout(
                            IPC_TIMEOUT,
                            read_framed_message::<_, DaemonRequest>(&mut stream),
                        )
                        .await;

                        match read_res {
                            Ok(Ok(request)) => {
                                let (resp_tx, resp_rx) = oneshot::channel();
                                let invocation = IpcInvocation {
                                    request,
                                    responder: resp_tx,
                                };
                                if sender.send(invocation).await.is_ok() {
                                    let response = match timeout(IPC_TIMEOUT, resp_rx).await {
                                        Ok(Ok(resp)) => resp,
                                        _ => DaemonResponse::Error("业务处理超时".to_string()),
                                    };
                                    let _ = timeout(
                                        IPC_TIMEOUT,
                                        write_framed_message(&mut stream, &response),
                                    )
                                    .await;
                                } else {
                                    let _ = timeout(
                                        IPC_TIMEOUT,
                                        write_framed_message(
                                            &mut stream,
                                            &DaemonResponse::Error(
                                                "守护进程主循环已关闭".to_string(),
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
                Err(e) => {
                    tracing::warn!("IPC 连接接入异常: {e}");
                }
            }
        }
    }
}

impl Drop for IpcListener {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
            tracing::debug!("已清理 IPC 套接字文件: {:?}", self.socket_path);
        }
    }
}
