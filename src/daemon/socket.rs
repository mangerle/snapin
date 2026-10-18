//! Unix Domain Socket 客户端与服务端通信实现。
//!
//! 提供单实例进程锁检测、CLI 指令毫秒级路由转发与守护服务端异步事件监听。

use crate::error::SnapinError;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc::Sender;

/// IPC 支持的守护进程控制命令枚举
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonCommand {
    /// 触发区域截图流水线
    Snip,
    /// 触发贴图（选区或剪贴板）
    Paste,
    /// 批量隐藏或显示当前所有贴图
    TogglePasters,
    /// 切换活动贴图分组
    SwitchGroup(usize),
    /// 心跳与存活检测
    Ping,
}

impl DaemonCommand {
    /// 将命令编码为纯文本协议字符串
    pub fn to_wire(&self) -> String {
        match self {
            Self::Snip => "SNIP\n".to_string(),
            Self::Paste => "PASTE\n".to_string(),
            Self::TogglePasters => "TOGGLE\n".to_string(),
            Self::SwitchGroup(id) => format!("SWITCH_GROUP:{id}\n"),
            Self::Ping => "PING\n".to_string(),
        }
    }

    /// 从纯文本协议行中解码出对应的命令
    pub fn from_wire(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed == "SNIP" {
            Some(Self::Snip)
        } else if trimmed == "PASTE" {
            Some(Self::Paste)
        } else if trimmed == "TOGGLE" {
            Some(Self::TogglePasters)
        } else if trimmed == "PING" {
            Some(Self::Ping)
        } else if let Some(id_str) = trimmed.strip_prefix("SWITCH_GROUP:") {
            id_str.parse::<usize>().ok().map(Self::SwitchGroup)
        } else {
            None
        }
    }
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

/// 客户端角色：向正在运行的守护进程发送指令。
///
/// 若成功连接并发送指令则返回 `Ok(true)`；
/// 若套接字不存在或守护进程未启动，则返回 `Ok(false)`，调用方可据此决定拉起后台守护服务。
pub async fn send_command_to_daemon(command: &DaemonCommand) -> Result<bool, SnapinError> {
    let socket_path = get_socket_path();
    if !socket_path.exists() {
        return Ok(false);
    }

    let connect_res = UnixStream::connect(&socket_path).await;
    let mut stream = match connect_res {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            // 套接字文件残留但无进程监听，清理残留文件
            let _ = std::fs::remove_file(&socket_path);
            return Ok(false);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(SnapinError::IpcError {
                socket_path: socket_path.to_string_lossy().to_string(),
                message: format!("连接守护进程失败: {e}"),
            })
        }
    };

    let wire_data = command.to_wire();
    stream
        .write_all(wire_data.as_bytes())
        .await
        .map_err(|e| SnapinError::IpcError {
            socket_path: socket_path.to_string_lossy().to_string(),
            message: format!("向守护进程发送数据失败: {e}"),
        })?;

    Ok(true)
}

/// 服务端角色：绑定并监听 Unix Domain Socket，将收到的命令通过有界通道派发给事件主循环
pub struct IpcListener {
    socket_path: PathBuf,
    listener: UnixListener,
}

impl IpcListener {
    /// 绑定套接字并初始化监听器，若检测到已有存活实例则拒绝重复启动，若为残留文件则自动清理
    pub async fn bind(custom_path: Option<PathBuf>) -> Result<Self, SnapinError> {
        let socket_path = custom_path.unwrap_or_else(get_socket_path);

        if socket_path.exists() {
            if let Ok(mut stream) = UnixStream::connect(&socket_path).await {
                if stream.write_all(b"PING\n").await.is_ok() {
                    return Err(SnapinError::IpcError {
                        socket_path: socket_path.to_string_lossy().to_string(),
                        message: "已有另一个 Snapin 守护进程在运行中".to_string(),
                    });
                }
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

    /// 启动常驻异步监听循环，将解码出的合法命令推送至有界通道
    pub async fn run_loop(self, cmd_sender: Sender<DaemonCommand>) {
        loop {
            match self.listener.accept().await {
                Ok((mut stream, _addr)) => {
                    let sender = cmd_sender.clone();
                    tokio::spawn(async move {
                        let mut buffer = [0u8; 256];
                        if let Ok(n) = stream.read(&mut buffer).await {
                            if n > 0 {
                                if let Ok(text) = std::str::from_utf8(&buffer[..n]) {
                                    if let Some(cmd) = DaemonCommand::from_wire(text) {
                                        let _ = sender.send(cmd).await;
                                    }
                                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_command_wire_format() {
        assert_eq!(DaemonCommand::Snip.to_wire(), "SNIP\n");
        assert_eq!(
            DaemonCommand::from_wire("SNIP\n"),
            Some(DaemonCommand::Snip)
        );

        assert_eq!(DaemonCommand::SwitchGroup(3).to_wire(), "SWITCH_GROUP:3\n");
        assert_eq!(
            DaemonCommand::from_wire("SWITCH_GROUP:3"),
            Some(DaemonCommand::SwitchGroup(3))
        );

        assert_eq!(DaemonCommand::from_wire("INVALID_COMMAND"), None);
    }
}
