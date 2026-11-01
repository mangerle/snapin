//! IPC 消息的分帧序列化与反序列化编解码器。
//!
//! 帧头固定为 `magic(4) + version(1) + reserved(3) + length(4)`。版本号让
//! 新旧 CLI/守护进程能在解析 JSON 前就给出明确的“协议不匹配”错误，而不是
//! 把任意二进制当成 JSON 后报告难以定位的语法错误。

use super::protocol::MAX_IPC_FRAME_SIZE;
use crate::error::DaemonError;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// IPC 帧魔数
pub const IPC_MAGIC: [u8; 4] = *b"SNPI";
/// 当前 IPC 协议版本
pub const IPC_PROTOCOL_VERSION: u8 = 1;
/// 帧头长度：magic(4) + version(1) + reserved(3) + length(4)
const FRAME_HEADER_LEN: usize = 12;

/// 从异步读取流中读取一个带版本头的分帧消息并反序列化。
pub async fn read_framed_message<R: AsyncReadExt + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T, DaemonError> {
    let mut header = [0u8; FRAME_HEADER_LEN];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|source| DaemonError::Io {
            stage: "读取分帧帧头",
            source,
        })?;

    if header[0..4] != IPC_MAGIC {
        return Err(DaemonError::Protocol(
            "IPC 帧魔数不匹配，可能连接到了非 Snapin 服务或旧版协议".to_string(),
        ));
    }
    let version = header[4];
    if version != IPC_PROTOCOL_VERSION {
        return Err(DaemonError::Protocol(format!(
            "IPC 协议版本不匹配，客户端/服务端版本: {version}/{}",
            IPC_PROTOCOL_VERSION
        )));
    }

    let len_bytes: [u8; 4] = header[8..12]
        .try_into()
        .map_err(|_| DaemonError::Protocol("IPC 帧长度字段解析失败".to_string()))?;
    let len = u32::from_be_bytes(len_bytes) as usize;
    if len == 0 || len > MAX_IPC_FRAME_SIZE {
        return Err(DaemonError::Protocol(format!(
            "分帧负载长度非法: {len} 字节（上限 {MAX_IPC_FRAME_SIZE} 字节）"
        )));
    }

    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|source| DaemonError::Io {
            stage: "读取分帧负载",
            source,
        })?;

    let val = serde_json::from_slice(&buf)?;
    Ok(val)
}

/// 将消息序列化为 JSON，并追加带版本号的帧头写入异步输出流。
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

    let mut header = [0u8; FRAME_HEADER_LEN];
    header[0..4].copy_from_slice(&IPC_MAGIC);
    header[4] = IPC_PROTOCOL_VERSION;
    // header[5..8] 为保留字段，保持全零以便未来扩展
    header[8..12].copy_from_slice(&(len as u32).to_be_bytes());

    writer
        .write_all(&header)
        .await
        .map_err(|source| DaemonError::Io {
            stage: "写入分帧帧头",
            source,
        })?;
    writer
        .write_all(&payload)
        .await
        .map_err(|source| DaemonError::Io {
            stage: "写入分帧负载",
            source,
        })?;
    writer.flush().await.map_err(|source| DaemonError::Io {
        stage: "刷写分帧缓冲",
        source,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧版 4 字节长度前缀必须被明确拒绝，而不是误读成 JSON。
    #[tokio::test]
    async fn test_legacy_length_only_frame_is_rejected() {
        let (mut client, mut server) = tokio::io::duplex(64);
        client
            .write_all(&2u32.to_be_bytes())
            .await
            .expect("写入旧版帧头应成功");
        client.write_all(b"{}").await.expect("写入旧版负载应成功");
        // 补足固定帧头长度，让读端先完成帧头读取再进入魔数校验。
        client.write_all(&[0u8; 6]).await.expect("补足帧头应成功");
        drop(client);

        let error = read_framed_message::<_, serde_json::Value>(&mut server)
            .await
            .expect_err("旧版帧必须被拒绝");
        assert!(
            matches!(error, DaemonError::Protocol(_)),
            "应返回协议错误，实际: {error:?}"
        );
    }

    /// 版本号不匹配时必须给出可定位的错误。
    #[tokio::test]
    async fn test_version_mismatch_is_rejected() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let mut header = [0u8; FRAME_HEADER_LEN];
        header[0..4].copy_from_slice(&IPC_MAGIC);
        header[4] = IPC_PROTOCOL_VERSION.wrapping_add(1);
        header[8..12].copy_from_slice(&2u32.to_be_bytes());
        client.write_all(&header).await.expect("写入帧头应成功");
        client.write_all(b"{}").await.expect("写入负载应成功");

        let error = read_framed_message::<_, serde_json::Value>(&mut server)
            .await
            .expect_err("版本不匹配必须被拒绝");
        assert!(
            matches!(error, DaemonError::Protocol(_)),
            "应返回协议错误，实际: {error:?}"
        );
    }
}
