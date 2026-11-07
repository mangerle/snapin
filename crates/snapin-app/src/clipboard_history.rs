//! 剪贴板历史监控。
//!
//! 使用固定间隔轮询读取系统剪贴板，读写与哈希计算放入阻塞线程池。相同内容不会
//! 重复入库，历史仓库负责 10 条与 1 天保留策略。

use anyhow::{Context, Result};
use image::RgbaImage;
use sha2::{Digest, Sha256};
use snapin_config::config::HexColor;
use snapin_platform::{read_clipboard_image, read_clipboard_text};
use snapin_runtime::{TaskPool, TaskRuntime, TaskRuntimeError};
use snapin_storage::{append_clipboard_image, append_clipboard_text, ClipboardEntryKind};
use std::time::{Duration, Instant};
#[cfg(not(target_os = "windows"))]
use tokio::time::MissedTickBehavior;

/// 剪贴板轮询间隔。
///
/// 平台没有提供剪贴板变更事件，只能轮询；图像剪贴板每次读取都要解码并计算
/// 内容哈希，过短间隔会持续占用阻塞线程。一秒一次在历史记录的实时性与空闲
/// 开销之间取平衡，内容未变化时只做一次读取与比较，不触发落盘。
#[cfg(not(target_os = "windows"))]
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// 持久化失败后的重试间隔，避免磁盘故障时每 500 毫秒重复告警。
const PERSIST_RETRY_DELAY: Duration = Duration::from_secs(30);

/// 最近一次读取到的剪贴板内容。
#[derive(Clone)]
enum ClipboardContent {
    /// 文本或颜色值。
    Text {
        /// 原始文本。
        text: String,
        /// 截屏历史条目类别。
        kind: ClipboardEntryKind,
    },
    /// 图像内容。
    Image(RgbaImage),
}

/// 启动剪贴板历史监控任务。
///
/// # Errors
///
/// 统一后台运行时拒绝任务时返回错误。
pub(crate) fn run_clipboard_history_monitor(runtime: &TaskRuntime) -> Result<()> {
    let cancel = runtime.cancellation_token();
    let runtime = runtime.clone();
    runtime
        .clone()
        .spawn_in(TaskPool::Background, async move {
            let mut persist_gate = PersistGate::default();
            #[cfg(target_os = "windows")]
            let (_guard, mut clipboard_rx) = snapin_platform::subscribe_clipboard_changes();
            #[cfg(not(target_os = "windows"))]
            let mut interval = tokio::time::interval(POLL_INTERVAL);
            #[cfg(not(target_os = "windows"))]
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                #[cfg(target_os = "windows")]
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    event = clipboard_rx.recv() => {
                        if event.is_none() {
                            break;
                        }
                    }
                }

                #[cfg(not(target_os = "windows"))]
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {}
                }

                #[cfg(target_os = "windows")]
                tokio::time::sleep(Duration::from_millis(60)).await;

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
                if !persist_gate.should_attempt(&fingerprint, Instant::now()) {
                    continue;
                }
                let persist =
                    runtime.run_blocking_in(TaskPool::Io, move || persist_snapshot(snapshot));
                match persist.await {
                    Ok(Ok(())) => persist_gate.record_success(&fingerprint),
                    Ok(Err(error)) => {
                        tracing::warn!("保存剪贴板历史失败: {error:#}");
                        persist_gate.record_failure(&fingerprint, Instant::now());
                    }
                    Err(TaskRuntimeError::ShuttingDown) => break,
                    Err(error) => {
                        tracing::warn!("保存剪贴板历史任务未完成: {error}");
                        persist_gate.record_failure(&fingerprint, Instant::now());
                    }
                }
            }
        })
        .map(|_| ())
        .context("提交剪贴板历史监控任务失败")
}

/// 控制同一剪贴板快照的持久化重试节奏。
///
/// 失败快照在退避时间后重试，成功后不再重复处理；既避免临时磁盘故障导致内容
/// 永久丢失，也避免轮询循环持续刷写告警日志。
#[derive(Default)]
struct PersistGate {
    last_fingerprint: String,
    retry_at: Option<Instant>,
}

impl PersistGate {
    fn should_attempt(&self, fingerprint: &str, now: Instant) -> bool {
        fingerprint != self.last_fingerprint
            || self.retry_at.is_some_and(|retry_at| now >= retry_at)
    }

    fn record_success(&mut self, fingerprint: &str) {
        self.last_fingerprint.clear();
        self.last_fingerprint.push_str(fingerprint);
        self.retry_at = None;
    }

    fn record_failure(&mut self, fingerprint: &str, now: Instant) {
        self.last_fingerprint.clear();
        self.last_fingerprint.push_str(fingerprint);
        self.retry_at = Some(now + PERSIST_RETRY_DELAY);
    }
}

impl ClipboardContent {
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

fn persist_snapshot(snapshot: ClipboardContent) -> Result<()> {
    match snapshot {
        ClipboardContent::Text { text, kind } => {
            append_clipboard_text(&text, kind).map_err(anyhow::Error::new)?;
        }
        ClipboardContent::Image(image) => {
            append_clipboard_image(&image).map_err(anyhow::Error::new)?;
        }
    }
    Ok(())
}

fn read_snapshot() -> Option<(String, ClipboardContent)> {
    let snapshot = read_snapshot_value()?;
    let fingerprint = snapshot.fingerprint();
    Some((fingerprint, snapshot))
}

fn read_snapshot_value() -> Option<ClipboardContent> {
    if let Ok(Some(text)) = read_clipboard_text() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let kind = if HexColor::from_hex(trimmed).is_some() {
                ClipboardEntryKind::Color
            } else {
                ClipboardEntryKind::Text
            };
            return Some(ClipboardContent::Text { text, kind });
        }
    }
    read_clipboard_image()
        .ok()
        .flatten()
        .map(ClipboardContent::Image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persist_gate_retries_only_after_backoff() {
        let now = Instant::now();
        let mut gate = PersistGate::default();

        assert!(gate.should_attempt("same", now));
        gate.record_failure("same", now);
        assert!(!gate.should_attempt("same", now + Duration::from_secs(1)));
        assert!(gate.should_attempt("same", now + PERSIST_RETRY_DELAY));
        assert!(gate.should_attempt("changed", now));

        gate.record_success("same");
        assert!(!gate.should_attempt("same", now + PERSIST_RETRY_DELAY));
    }
}
