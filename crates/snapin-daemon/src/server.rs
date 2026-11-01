//! Unix Domain Socket 服务端监听器实现。

use super::client::send_request_to_path;
use super::codec::{read_framed_message, write_framed_message};
use super::protocol::{get_socket_path, DaemonRequest, DaemonResponse, IpcInvocation, IPC_TIMEOUT};
use crate::error::DaemonError;
use std::path::PathBuf;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::mpsc::Sender;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::timeout;

/// 同时处理的 IPC 连接上限，防止异常客户端耗尽任务与内存
const MAX_CONCURRENT_CONNECTIONS: usize = 32;

/// 服务端角色：绑定并监听 IPC 套接字/管道，将收到的命令分发给事件主循环
pub struct IpcListener {
    socket_path: PathBuf,
    #[cfg(unix)]
    listener: UnixListener,
    #[cfg(windows)]
    pipe_name: String,
    #[cfg(windows)]
    initial_server: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
}

impl IpcListener {
    /// 绑定套接字并初始化监听器，若检测到已有存活实例则拒绝重复启动，若为残留文件则自动清理
    pub async fn bind(custom_path: Option<PathBuf>) -> Result<Self, DaemonError> {
        let socket_path = custom_path.unwrap_or_else(get_socket_path);

        #[cfg(unix)]
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

        #[cfg(unix)]
        let listener = UnixListener::bind(&socket_path).map_err(|e| DaemonError::Socket {
            socket_path: socket_path.to_string_lossy().to_string(),
            message: format!("绑定 IPC 套接字失败: {e}"),
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(error) =
                std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
            {
                tracing::warn!(
                    "收紧 IPC 套接字权限失败，路径: {:?}，原因: {error}",
                    socket_path
                );
            }
        }

        #[cfg(windows)]
        let pipe_name = crate::client::windows_pipe_name(&socket_path);
        #[cfg(windows)]
        let initial_server = match tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .create(&pipe_name)
        {
            Ok(server) => server,
            Err(e) => {
                if let Ok(Some(DaemonResponse::Pong)) =
                    send_request_to_path(&socket_path, &DaemonRequest::Ping).await
                {
                    return Err(DaemonError::Socket {
                        socket_path: pipe_name,
                        message: "已有另一个 Snapin 守护进程在运行中".to_string(),
                    });
                }
                return Err(DaemonError::Socket {
                    socket_path: pipe_name,
                    message: format!("绑定 IPC 命名管道失败: {e}"),
                });
            }
        };

        tracing::info!("Snapin IPC 监听服务就绪，套接字路径: {:?}", socket_path);
        Ok(Self {
            socket_path,
            #[cfg(unix)]
            listener,
            #[cfg(windows)]
            pipe_name,
            #[cfg(windows)]
            initial_server: Some(initial_server),
        })
    }

    /// 启动常驻异步监听循环，将解码出的请求与响应回执派发给有界通道。
    pub async fn run_loop(
        #[allow(unused_mut)] mut self,
        cmd_sender: Sender<IpcInvocation>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let mut connections = JoinSet::new();

        #[cfg(unix)]
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
                accepted = self.listener.accept() => {
                    match accepted {
                        Ok((stream, _addr)) => {
                            spawn_connection_handler(stream, cmd_sender.clone(), &mut connections);
                        }
                        Err(e) => {
                            tracing::warn!("IPC 连接接入异常: {e}");
                        }
                    }
                }
            }
        }

        #[cfg(windows)]
        {
            let mut current_server = self.initial_server.take();
            loop {
                let server = match current_server.take() {
                    Some(s) => s,
                    None => match tokio::net::windows::named_pipe::ServerOptions::new()
                        .create(&self.pipe_name)
                    {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("创建命名管道实例失败: {e}");
                            break;
                        }
                    },
                };

                tokio::select! {
                    _ = shutdown.changed() => break,
                    Some(_) = connections.join_next(), if !connections.is_empty() => {
                        current_server = Some(server);
                    }
                    connected = server.connect() => {
                        match connected {
                            Ok(()) => {
                                spawn_connection_handler(server, cmd_sender.clone(), &mut connections);
                            }
                            Err(e) => {
                                tracing::warn!("命名管道连接接入异常: {e}");
                            }
                        }
                    }
                }
            }
        }

        connections.shutdown().await;
        tracing::info!("IPC 监听循环已退出");
    }
}

/// 为单个客户端连接生成异步处理任务，完成请求帧反序列化、业务调用与响应帧回传
fn spawn_connection_handler<S>(
    mut stream: S,
    sender: Sender<IpcInvocation>,
    connections: &mut JoinSet<()>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    if connections.len() >= MAX_CONCURRENT_CONNECTIONS {
        let _ = connections.try_join_next();
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
                let invocation = IpcInvocation {
                    request,
                    responder: resp_tx,
                };
                if sender.send(invocation).await.is_ok() {
                    let response = match timeout(IPC_TIMEOUT, resp_rx).await {
                        Ok(Ok(resp)) => resp,
                        _ => DaemonResponse::Error("业务处理超时".to_string()),
                    };
                    let _ =
                        timeout(IPC_TIMEOUT, write_framed_message(&mut stream, &response)).await;
                } else {
                    let _ = timeout(
                        IPC_TIMEOUT,
                        write_framed_message(
                            &mut stream,
                            &DaemonResponse::Error("守护进程主循环已关闭".to_string()),
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

impl Drop for IpcListener {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
            tracing::debug!("已清理 IPC 套接字文件: {:?}", self.socket_path);
        }
    }
}
