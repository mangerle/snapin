//! Unix Domain Socket 服务端监听器实现。

mod connection;

use super::client::send_request_to_path;
use super::protocol::{get_socket_path, DaemonRequest, DaemonResponse, IpcInvocation};
use crate::error::DaemonError;
use connection::spawn_connection_handler;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio::sync::mpsc::Sender;
use tokio::sync::watch;
use tokio::task::JoinSet;

/// 同时处理的 IPC 连接上限，防止异常客户端耗尽任务与内存
const MAX_CONCURRENT_CONNECTIONS: usize = 32;

/// 服务端角色：绑定并监听 IPC 套接字/管道，将收到的命令分发给事件主循环
pub struct IpcListener {
    socket_path: PathBuf,
    /// 单实例锁文件；持有期间禁止其他守护进程绑定同一路径。
    _lock_file: File,
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
        let lock_file = acquire_instance_lock(&socket_path)?;

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
            _lock_file: lock_file,
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
        self,
        cmd_sender: Sender<IpcInvocation>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        #[cfg(target_os = "windows")]
        let mut this = self;
        #[cfg(not(target_os = "windows"))]
        let this = self;
        let mut connections = JoinSet::new();

        #[cfg(unix)]
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                result = connections.join_next(), if !connections.is_empty() => {
                    if let Some(Err(error)) = result {
                        tracing::warn!("IPC 连接任务异常退出: {error}");
                    }
                }
                accepted = this.listener.accept() => {
                    match accepted {
                        Ok((stream, _addr)) => {
                            spawn_connection_handler(stream, cmd_sender.clone(), &mut connections).await;
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
            let mut current_server = this.initial_server.take();
            loop {
                let server = match current_server.take() {
                    Some(s) => s,
                    None => match tokio::net::windows::named_pipe::ServerOptions::new()
                        .create(&this.pipe_name)
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
                    result = connections.join_next(), if !connections.is_empty() => {
                        if let Some(Err(error)) = result {
                            tracing::warn!("IPC 连接任务异常退出: {error}");
                        }
                        current_server = Some(server);
                    }
                    connected = server.connect() => {
                        match connected {
                            Ok(()) => {
                                spawn_connection_handler(server, cmd_sender.clone(), &mut connections).await;
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

/// 获取跨进程单实例锁，覆盖 Ping 探测与残留套接字清理之间的竞态窗口。
///
/// 锁文件始终保留（解锁由进程退出或文件句柄释放完成），这样崩溃后的下一次启动
/// 可以立即复用同一锁文件；锁文件本身存在不代表实例存活，只有持有排他锁才代表。
fn acquire_instance_lock(socket_path: &std::path::Path) -> Result<File, DaemonError> {
    let lock_path = socket_path.with_extension("lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| DaemonError::Socket {
            socket_path: parent.to_string_lossy().to_string(),
            message: format!("创建单实例锁目录失败: {error}"),
        })?;
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| DaemonError::Socket {
            socket_path: lock_path.to_string_lossy().to_string(),
            message: format!("打开单实例锁文件失败: {error}"),
        })?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(DaemonError::Socket {
            socket_path: socket_path.to_string_lossy().to_string(),
            message: "已有另一个 Snapin 守护进程持有单实例锁".to_string(),
        }),
        Err(TryLockError::Error(error)) => Err(DaemonError::Socket {
            socket_path: lock_path.to_string_lossy().to_string(),
            message: format!("获取单实例锁失败: {error}"),
        }),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 单实例锁必须在 Ping 探测与套接字绑定之间形成原子互斥。
    #[tokio::test]
    async fn test_second_listener_is_rejected_by_instance_lock() {
        let dir = tempfile::tempdir().expect("应可创建测试目录");
        let socket_path = dir.path().join("snapin-test.sock");
        let first = IpcListener::bind(Some(socket_path.clone()))
            .await
            .expect("首个监听器应能绑定");
        let second = IpcListener::bind(Some(socket_path)).await;
        assert!(
            matches!(second, Err(DaemonError::Socket { .. })),
            "第二个监听器必须被单实例锁拒绝"
        );
        drop(first);
    }
}
