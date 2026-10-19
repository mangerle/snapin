//! Unix Domain Socket 客户端与服务端通信实现。
//!
//! 提供强类型双向请求-响应消息协议、带 4 字节大端长度前缀的分帧编解码器、
//! 单实例锁竞争自愈以及守护服务端全双工事件通信。

use serde::{Deserialize, Serialize};
use snapin_core::error::SnapinError;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::time::timeout;

/// 最大允许的 IPC 数据包尺寸（64MB，满足全屏高分图像在 Unix Socket 极速内存流式传输）
const MAX_IPC_FRAME_SIZE: usize = 64 * 1024 * 1024;
/// IPC 通信读写超时阈值（2 秒，避免套接字挂起导致的协程泄漏）
const IPC_TIMEOUT: Duration = Duration::from_secs(2);

/// IPC 支持的守护进程请求协议
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonRequest {
    /// 触发区域截图流水线
    Snip,
    /// 触发贴图（由剪贴板生成）
    Paste,
    /// 客户端直接向常驻守护进程投递像素数据创建贴图窗口
    CreatePaster {
        width: u32,
        height: u32,
        x: i32,
        y: i32,
        title: String,
        rgba: Vec<u8>,
    },
    /// 批量隐藏或显示当前所有贴图
    TogglePasters,
    /// 切换活动贴图分组（若为 None 则循环切换至下一个分组）
    SwitchGroup(Option<usize>),
    /// 请求守护进程优雅平滑停机
    Shutdown,
    /// 心跳与存活检测
    Ping,
    /// 查询守护进程当前运行状态
    GetStatus,
    /// 贴图窗口向守护进程注册长连接
    RegisterPaster { id: u64, group_id: usize },
    /// 贴图窗口注销通知
    UnregisterPaster(u64),
}

/// 兼容别名：保留 DaemonCommand 便于渐进过渡
pub type DaemonCommand = DaemonRequest;

/// 守护进程向贴图客户端下发的控制事件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasterControlEvent {
    /// 批量切换当前组贴图显示/隐藏
    ToggleVisibility,
    /// 显式设置可见性（用于切换贴图分组）
    SetVisibility(bool),
}

/// 守护进程向 IPC 客户端返回的强类型响应协议
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// 操作成功完成
    Ok,
    /// 心跳存活确认
    Pong,
    /// 贴图注册成功确认
    PasterRegistered,
    /// 状态查询结果快照
    Status {
        active_group: usize,
        paster_count: usize,
    },
    /// 操作执行异常反馈
    Error(String),
}

/// IPC 调度通道载荷：包含请求与用于回传响应的一对一 Oneshot 发送端
pub struct IpcInvocation {
    pub request: DaemonRequest,
    pub responder: oneshot::Sender<DaemonResponse>,
}

/// 从异步读取流中读取一个带 4 字节大端长度前缀的分帧消息并反序列化
pub async fn read_framed_message<R: AsyncReadExt + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T, SnapinError> {
    let mut len_bytes = [0u8; 4];
    reader
        .read_exact(&mut len_bytes)
        .await
        .map_err(|e| SnapinError::IpcError {
            socket_path: "stream".to_string(),
            message: format!("读取分帧长度头部失败: {e}"),
        })?;

    let len = u32::from_be_bytes(len_bytes) as usize;
    if len == 0 || len > MAX_IPC_FRAME_SIZE {
        return Err(SnapinError::IpcError {
            socket_path: "stream".to_string(),
            message: format!("分帧负载长度非法: {len} 字节（上限 {MAX_IPC_FRAME_SIZE} 字节）"),
        });
    }

    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| SnapinError::IpcError {
            socket_path: "stream".to_string(),
            message: format!("读取分帧负载失败: {e}"),
        })?;

    serde_json::from_slice(&buf).map_err(|e| SnapinError::IpcError {
        socket_path: "stream".to_string(),
        message: format!("反序列化分帧消息失败: {e}"),
    })
}

/// 将消息序列化为 JSON 并追加 4 字节大端长度前缀写入异步输出流
pub async fn write_framed_message<W: AsyncWriteExt + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> Result<(), SnapinError> {
    let payload = serde_json::to_vec(msg).map_err(|e| SnapinError::IpcError {
        socket_path: "stream".to_string(),
        message: format!("序列化分帧消息失败: {e}"),
    })?;

    let len = payload.len();
    if len > MAX_IPC_FRAME_SIZE {
        return Err(SnapinError::IpcError {
            socket_path: "stream".to_string(),
            message: format!("发送消息过大: {len} 字节（上限 {MAX_IPC_FRAME_SIZE} 字节）"),
        });
    }

    writer
        .write_all(&(len as u32).to_be_bytes())
        .await
        .map_err(|e| SnapinError::IpcError {
            socket_path: "stream".to_string(),
            message: format!("写入分帧长度失败: {e}"),
        })?;

    writer
        .write_all(&payload)
        .await
        .map_err(|e| SnapinError::IpcError {
            socket_path: "stream".to_string(),
            message: format!("写入分帧负载失败: {e}"),
        })?;

    writer.flush().await.map_err(|e| SnapinError::IpcError {
        socket_path: "stream".to_string(),
        message: format!("刷新分帧输出流失败: {e}"),
    })?;

    Ok(())
}

/// 获取本用户的默认 IPC Socket 路径：优先 `$XDG_RUNTIME_DIR/snapin.sock`，兜底 `/tmp/snapin-$UID.sock`
pub fn get_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("snapin.sock")
    } else {
        let uid = rustix::process::getuid().as_raw();
        PathBuf::from(format!("/tmp/snapin-{uid}.sock"))
    }
}

/// 客户端角色：向正在运行的守护进程发送强类型请求，并等待接收双向确认响应。
///
/// 若守护进程未运行（Socket 不存在或无进程监听），返回 `Ok(None)`；
/// 若发送并接收到响应，返回 `Ok(Some(response))`；
/// 若通信超时或套接字损坏，返回相应的 `Err(SnapinError)`。
pub async fn send_request_to_daemon(
    request: &DaemonRequest,
) -> Result<Option<DaemonResponse>, SnapinError> {
    let socket_path = get_socket_path();
    send_request_to_path(&socket_path, request).await
}

/// 向指定路径的套接字发送请求并等待响应（便于单元测试隔离）
pub async fn send_request_to_path(
    socket_path: &Path,
    request: &DaemonRequest,
) -> Result<Option<DaemonResponse>, SnapinError> {
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
            return Err(SnapinError::IpcError {
                socket_path: socket_path.to_string_lossy().to_string(),
                message: format!("连接守护进程失败: {e}"),
            })
        }
        Err(_) => {
            return Err(SnapinError::IpcError {
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
        Err(_) => Err(SnapinError::IpcError {
            socket_path: socket_path.to_string_lossy().to_string(),
            message: "等待守护进程响应超时".to_string(),
        }),
    }
}

/// 兼容旧版调用的客户端辅助函数：发送指令并判断守护进程是否成功响应
pub async fn send_command_to_daemon(command: &DaemonRequest) -> Result<bool, SnapinError> {
    match send_request_to_daemon(command).await? {
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

/// 服务端角色：绑定并监听 Unix Domain Socket，将收到的命令分发给事件主循环
pub struct IpcListener {
    socket_path: PathBuf,
    listener: UnixListener,
}

impl IpcListener {
    /// 绑定套接字并初始化监听器，若检测到已有存活实例则拒绝重复启动，若为残留文件则自动清理
    pub async fn bind(custom_path: Option<PathBuf>) -> Result<Self, SnapinError> {
        let socket_path = custom_path.unwrap_or_else(get_socket_path);

        if socket_path.exists() {
            // 发送 Ping 试探已有实例是否真实存活
            if let Ok(Some(DaemonResponse::Pong)) =
                send_request_to_path(&socket_path, &DaemonRequest::Ping).await
            {
                return Err(SnapinError::IpcError {
                    socket_path: socket_path.to_string_lossy().to_string(),
                    message: "已有另一个 Snapin 守护进程在运行中".to_string(),
                });
            }
            let _ = std::fs::remove_file(&socket_path);
        }

        let listener = UnixListener::bind(&socket_path).map_err(|e| SnapinError::IpcError {
            socket_path: socket_path.to_string_lossy().to_string(),
            message: format!("绑定 IPC 套接字失败: {e}"),
        })?;

        tracing::info!("Snapin IPC 监听服务就绪，套接字路径: {:?}", socket_path);
        Ok(Self {
            socket_path,
            listener,
        })
    }

    /// 启动常驻异步监听循环，将解码出的请求与响应回执派发给有界通道，并支持贴图长连接广播控制
    pub async fn run_loop(
        self,
        cmd_sender: Sender<IpcInvocation>,
        paster_broadcast_tx: broadcast::Sender<PasterControlEvent>,
    ) {
        loop {
            match self.listener.accept().await {
                Ok((mut stream, _addr)) => {
                    let sender = cmd_sender.clone();
                    let b_tx = paster_broadcast_tx.clone();
                    tokio::spawn(async move {
                        match read_framed_message::<_, DaemonRequest>(&mut stream).await {
                            Ok(DaemonRequest::RegisterPaster { id, group_id }) => {
                                let (resp_tx, resp_rx) = oneshot::channel();
                                let invocation = IpcInvocation {
                                    request: DaemonRequest::RegisterPaster { id, group_id },
                                    responder: resp_tx,
                                };
                                if sender.send(invocation).await.is_ok() {
                                    if let Ok(Ok(response)) = timeout(IPC_TIMEOUT, resp_rx).await {
                                        let _ = write_framed_message(&mut stream, &response).await;
                                    }
                                }

                                // 转入长连接控制指令接收循环
                                let mut sub_rx = b_tx.subscribe();
                                loop {
                                    tokio::select! {
                                        ev_res = sub_rx.recv() => {
                                            match ev_res {
                                                Ok(ev) => {
                                                    if write_framed_message(&mut stream, &ev).await.is_err() {
                                                        break;
                                                    }
                                                }
                                                Err(broadcast::error::RecvError::Lagged(_)) => {}
                                                Err(broadcast::error::RecvError::Closed) => break,
                                            }
                                        }
                                        read_res = read_framed_message::<_, DaemonRequest>(&mut stream) => {
                                            match read_res {
                                                Ok(DaemonRequest::UnregisterPaster(_)) | Err(_) => {
                                                    break;
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }

                                // 客户端连接退出，通知守护进程注销实例
                                let (unreg_tx, _) = oneshot::channel();
                                let _ = sender
                                    .send(IpcInvocation {
                                        request: DaemonRequest::UnregisterPaster(id),
                                        responder: unreg_tx,
                                    })
                                    .await;
                            }
                            Ok(request) => {
                                let (resp_tx, resp_rx) = oneshot::channel();
                                let invocation = IpcInvocation {
                                    request,
                                    responder: resp_tx,
                                };
                                if sender.send(invocation).await.is_ok() {
                                    if let Ok(Ok(response)) = timeout(IPC_TIMEOUT, resp_rx).await {
                                        let _ = write_framed_message(&mut stream, &response).await;
                                    } else {
                                        let _ = write_framed_message(
                                            &mut stream,
                                            &DaemonResponse::Error("业务处理超时".to_string()),
                                        )
                                        .await;
                                    }
                                } else {
                                    let _ = write_framed_message(
                                        &mut stream,
                                        &DaemonResponse::Error("守护进程主循环已关闭".to_string()),
                                    )
                                    .await;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("接收客户端 IPC 请求异常: {e}");
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

/// 贴图窗口与守护进程长连接监听实时控制事件
pub async fn listen_paster_events<F>(paster_id: u64, group_id: usize, mut on_event: F)
where
    F: FnMut(PasterControlEvent) + Send + 'static,
{
    let socket_path = get_socket_path();
    if !socket_path.exists() {
        return;
    }

    let Ok(Ok(mut stream)) = timeout(IPC_TIMEOUT, UnixStream::connect(&socket_path)).await else {
        return;
    };

    let req = DaemonRequest::RegisterPaster {
        id: paster_id,
        group_id,
    };
    if write_framed_message(&mut stream, &req).await.is_err() {
        return;
    }

    let Ok(Ok(DaemonResponse::PasterRegistered)) = timeout(
        IPC_TIMEOUT,
        read_framed_message::<_, DaemonResponse>(&mut stream),
    )
    .await
    else {
        return;
    };

    while let Ok(event) = read_framed_message::<_, PasterControlEvent>(&mut stream).await {
        on_event(event);
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

    #[tokio::test]
    async fn test_framed_message_roundtrip() {
        let (mut client, mut server) = tokio::io::duplex(1024);

        let req = DaemonRequest::SwitchGroup(Some(42));
        tokio::spawn(async move {
            write_framed_message(&mut client, &req)
                .await
                .expect("写入分帧请求成功");
            let resp: DaemonResponse = read_framed_message(&mut client)
                .await
                .expect("读取分帧响应成功");
            assert_eq!(resp, DaemonResponse::Ok);
        });

        let received: DaemonRequest = read_framed_message(&mut server)
            .await
            .expect("服务端读取请求成功");
        assert_eq!(received, DaemonRequest::SwitchGroup(Some(42)));
        write_framed_message(&mut server, &DaemonResponse::Ok)
            .await
            .expect("服务端写入响应成功");
    }

    #[tokio::test]
    async fn test_ipc_listener_and_client_dialogue() {
        let temp_dir = tempfile::tempdir().expect("创建临时目录成功");
        let sock_path = temp_dir.path().join("test_daemon.sock");

        let listener = IpcListener::bind(Some(sock_path.clone()))
            .await
            .expect("绑定测试套接字成功");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<IpcInvocation>(4);
        let (b_tx, _) = tokio::sync::broadcast::channel::<PasterControlEvent>(8);
        let b_tx_clone = b_tx.clone();

        tokio::spawn(async move {
            listener.run_loop(tx, b_tx_clone).await;
        });

        // 模拟守护进程主循环响应逻辑
        tokio::spawn(async move {
            while let Some(inv) = rx.recv().await {
                match inv.request {
                    DaemonRequest::Ping => {
                        let _ = inv.responder.send(DaemonResponse::Pong);
                    }
                    DaemonRequest::GetStatus => {
                        let _ = inv.responder.send(DaemonResponse::Status {
                            active_group: 1,
                            paster_count: 5,
                        });
                    }
                    DaemonRequest::RegisterPaster { .. } => {
                        let _ = inv.responder.send(DaemonResponse::PasterRegistered);
                    }
                    _ => {
                        let _ = inv.responder.send(DaemonResponse::Ok);
                    }
                }
            }
        });

        // 客户端发起 Ping
        let res = send_request_to_path(&sock_path, &DaemonRequest::Ping)
            .await
            .expect("Ping 通信成功");
        assert_eq!(res, Some(DaemonResponse::Pong));

        // 客户端发起 GetStatus
        let res = send_request_to_path(&sock_path, &DaemonRequest::GetStatus)
            .await
            .expect("GetStatus 通信成功");
        assert_eq!(
            res,
            Some(DaemonResponse::Status {
                active_group: 1,
                paster_count: 5
            })
        );
    }
}
