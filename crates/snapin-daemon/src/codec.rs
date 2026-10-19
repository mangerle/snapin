//! IPC 消息的分帧序列化与反序列化编解码器。

use super::protocol::MAX_IPC_FRAME_SIZE;
use serde::{Deserialize, Serialize};
use snapin_core::error::SnapinError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
