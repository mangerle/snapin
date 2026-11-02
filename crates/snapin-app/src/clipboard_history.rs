//! 剪贴板历史监控。
//!
//! 使用固定间隔轮询读取系统剪贴板，读写与哈希计算放入阻塞线程池。相同内容不会
//! 重复入库，历史仓库负责 10 条与 1 天保留策略。

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use snapin_core::config::HexColor;
use snapin_platform::{read_clipboard_image, read_clipboard_text};
use snapin_runtime::{TaskPool, TaskRuntime};
use snapin_storage::{append_clipboard_image, append_clipboard_text, ClipboardEntryKind};
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// 启动剪贴板历史监控任务。
///
/// # Errors
///
/// 统一后台运行时拒绝任务时返回错误。
pub fn run_clipboard_history_monitor(runtime: &TaskRuntime) -> Result<()> {
    let cancel = runtime.cancellation_token();
    runtime
        .spawn_in(TaskPool::Media, async move {
            let mut last_fingerprint = String::new();
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(POLL_INTERVAL) => {}
                }
                let snapshot = tokio::task::spawn_blocking(read_snapshot).await;
                let Ok(Some(snapshot)) = snapshot else {
                    continue;
                };
                let fingerprint = snapshot.fingerprint();
                if fingerprint == last_fingerprint {
                    continue;
                }
                last_fingerprint = fingerprint;
                if let Err(error) = snapshot.persist().await {
                    tracing::warn!("保存剪贴板历史失败: {error:#}");
                }
            }
        })
        .map(|_| ())
        .context("提交剪贴板历史监控任务失败")
}

enum ClipboardSnapshot {
    Text {
        text: String,
        kind: ClipboardEntryKind,
    },
    Image(image::RgbaImage),
}

impl ClipboardSnapshot {
    fn fingerprint(&self) -> String {
        match self {
            Self::Text { text, kind } => {
                let mut hasher = Sha256::new();
                hasher.update(format!("{kind:?}:{text}").as_bytes());
                format!("{:x}", hasher.finalize())
            }
            Self::Image(image) => {
                let mut hasher = Sha256::new();
                hasher.update(image.width().to_le_bytes());
                hasher.update(image.height().to_le_bytes());
                hasher.update(image.as_raw());
                format!("{:x}", hasher.finalize())
            }
        }
    }

    async fn persist(self) -> Result<()> {
        tokio::task::spawn_blocking(move || match self {
            Self::Text { text, kind } => append_clipboard_text(&text, kind).map(|_| ()),
            Self::Image(image) => append_clipboard_image(&image).map(|_| ()),
        })
        .await
        .context("剪贴板历史写入任务异常")?
        .map_err(anyhow::Error::new)
    }
}

fn read_snapshot() -> Option<ClipboardSnapshot> {
    if let Ok(Some(text)) = read_clipboard_text() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let kind = if HexColor::from_hex(trimmed).is_some() {
                ClipboardEntryKind::Color
            } else {
                ClipboardEntryKind::Text
            };
            return Some(ClipboardSnapshot::Text { text, kind });
        }
    }
    read_clipboard_image()
        .ok()
        .flatten()
        .map(ClipboardSnapshot::Image)
}
