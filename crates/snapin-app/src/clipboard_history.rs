//! 剪贴板历史监控。
//!
//! 使用固定间隔轮询读取系统剪贴板，读写与哈希计算放入阻塞线程池。相同内容不会
//! 重复入库，历史仓库负责 10 条与 1 天保留策略。

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use snapin_core::config::HexColor;
use snapin_platform::{read_clipboard_image, read_clipboard_text};
use snapin_runtime::{TaskPool, TaskRuntime, TaskRuntimeError};
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
    let runtime = runtime.clone();
    runtime
        .clone()
        .spawn_in(TaskPool::Maintenance, async move {
            let mut last_fingerprint = String::new();
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(POLL_INTERVAL) => {}
                }
                let read = runtime.run_blocking_in(TaskPool::Io, read_snapshot);
                let (fingerprint, snapshot) = tokio::select! {
                    _ = cancel.cancelled() => break,
                    result = read => match result {
                        Ok(Some(snapshot)) => snapshot,
                        Ok(None) => continue,
                        Err(TaskRuntimeError::ShuttingDown) => break,
                        Err(error) => {
                            tracing::debug!("读取剪贴板快照任务未完成: {error}");
                            continue;
                        }
                    },
                };
                if fingerprint == last_fingerprint {
                    continue;
                }
                last_fingerprint = fingerprint;
                let persist =
                    runtime.run_blocking_in(TaskPool::Io, move || persist_snapshot(snapshot));
                match persist.await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => tracing::warn!("保存剪贴板历史失败: {error:#}"),
                    Err(error) => tracing::warn!("保存剪贴板历史任务未完成: {error}"),
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
}

fn persist_snapshot(snapshot: ClipboardSnapshot) -> Result<()> {
    match snapshot {
        ClipboardSnapshot::Text { text, kind } => {
            append_clipboard_text(&text, kind).map_err(anyhow::Error::new)?;
        }
        ClipboardSnapshot::Image(image) => {
            append_clipboard_image(&image).map_err(anyhow::Error::new)?;
        }
    }
    Ok(())
}

fn read_snapshot() -> Option<(String, ClipboardSnapshot)> {
    let snapshot = read_snapshot_value()?;
    let fingerprint = snapshot.fingerprint();
    Some((fingerprint, snapshot))
}

fn read_snapshot_value() -> Option<ClipboardSnapshot> {
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
