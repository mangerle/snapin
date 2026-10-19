//! IPC 消息的分帧序列化与反序列化编解码器。

use super::protocol::MAX_IPC_FRAME_SIZE;
use crate::error::DaemonError;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 从异步读取流中读取一个带 4 字节大端长度前缀的分帧消息并反序列化
pub async fn read_framed_message<R: AsyncReadExt + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T, DaemonError> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).await?;

    let len = u32::from_be_bytes(len_bytes) as usize;
    if len == 0 || len > MAX_IPC_FRAME_SIZE {
        return Err(DaemonError::Protocol(format!(
            "分帧负载长度非法: {len} 字节（上限 {MAX_IPC_FRAME_SIZE} 字节）"
        )));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;

    let val = serde_json::from_slice(&buf)?;
    Ok(val)
}

/// 将消息序列化为 JSON 并追加 4 字节大端长度前缀写入异步输出流
pub async fn write_framed_message<W: AsyncWriteExt + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> Result<(), DaemonError> {
    let payload = serde_json::to_vec(msg)?;

    let len = payload.len();
    if len > MAX_IPC_FRAME_SIZE {
        return Err(DaemonError::Protocol(format!(
            "发送消息过大: {len} 字节（上限 {MAX_IPC_FRAME_SIZE} 字节）"
        )));
    }

    writer.write_all(&(len as u32).to_be_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;

    Ok(())
}
