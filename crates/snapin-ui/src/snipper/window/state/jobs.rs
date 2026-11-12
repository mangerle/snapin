//! 截图会话的异步媒体与剪贴板任务状态。

use super::super::clipboard::PendingClipboardJob;
use super::super::jobs::{SnipperJobKind, SnipperJobRuntime};
use crate::ports::{
    noop_clipboard_event_sink, noop_screen_capture_event_sink, ClipboardEventSink, JobCancel,
    JobId, ScreenCaptureEventSink,
};
use std::sync::Arc;

/// 当前在途媒体任务的完整快照。
pub(crate) struct PendingMediaJob {
    /// 任务类型。
    pub(crate) kind: SnipperJobKind,
    /// 执行器分配的任务标识。
    pub(crate) id: JobId,
    /// 协作取消闭包。
    pub(crate) cancel: JobCancel,
    /// 贴图任务的落点；仅 `Pin` 任务使用，其余任务为 `None`。
    pub(crate) pin_position: Option<(i32, i32)>,
}

/// 截图会话的异步任务与完成回传状态。
pub(crate) struct SnipperJobs {
    /// 媒体任务运行时。
    pub(crate) runtime: Option<Arc<SnipperJobRuntime>>,
    /// 当前在途媒体任务。
    pub(crate) pending_media: Option<PendingMediaJob>,
    /// 当前在途剪贴板写入。
    pub(crate) pending_clipboard: Option<PendingClipboardJob>,
    /// 剪贴板完成事件投递器。
    pub(crate) clipboard_events: ClipboardEventSink,
    /// 长截图抓屏完成事件投递器。
    pub(crate) screen_capture_events: ScreenCaptureEventSink,
    /// 当前在途长截图抓屏请求标识。
    pending_screen_capture: Option<u64>,
    /// 下一个抓屏请求标识；用于丢弃截图会话重建后才返回的陈旧结果。
    next_screen_capture_id: u64,
}

impl SnipperJobs {
    /// 构建没有在途任务的任务状态。
    pub(crate) fn new() -> Self {
        Self {
            runtime: None,
            pending_media: None,
            pending_clipboard: None,
            clipboard_events: noop_clipboard_event_sink(),
            screen_capture_events: noop_screen_capture_event_sink(),
            pending_screen_capture: None,
            next_screen_capture_id: 1,
        }
    }

    /// 登记一次抓屏请求；已有请求在途时返回 `None`，避免滚轮高频触发抓屏竞争。
    pub(crate) fn begin_screen_capture(&mut self) -> Option<u64> {
        if self.pending_screen_capture.is_some() {
            return None;
        }
        let request_id = self.next_screen_capture_id;
        self.next_screen_capture_id = self.next_screen_capture_id.saturating_add(1);
        self.pending_screen_capture = Some(request_id);
        Some(request_id)
    }

    /// 完成指定抓屏请求；陈旧或未知请求不会清除当前在途状态。
    pub(crate) fn finish_screen_capture(&mut self, request_id: u64) -> bool {
        if self.pending_screen_capture != Some(request_id) {
            return false;
        }
        self.pending_screen_capture = None;
        true
    }

    /// 是否已有抓屏请求在途。
    pub(crate) fn has_pending_screen_capture(&self) -> bool {
        self.pending_screen_capture.is_some()
    }

    /// 检查给定请求序号是否仍为当前唯一在途抓屏请求（不消耗该请求）。
    pub(crate) fn is_active_screen_capture(&self, request_id: u64) -> bool {
        self.pending_screen_capture == Some(request_id)
    }

    /// 丢弃在途抓屏请求标识，使随后返回的陈旧结果无法再修改已结束的截图会话。
    pub(crate) fn cancel_screen_capture(&mut self) {
        self.pending_screen_capture = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_capture_requests_are_serialized() {
        let mut jobs = SnipperJobs::new();
        let first = jobs.begin_screen_capture();
        assert!(first.is_some());
        assert!(jobs.begin_screen_capture().is_none());
        assert!(jobs.finish_screen_capture(first.unwrap_or_default()));
        assert!(!jobs.has_pending_screen_capture());
        assert!(jobs.begin_screen_capture().is_some());
    }

    #[test]
    fn test_stale_screen_capture_result_is_ignored() {
        let mut jobs = SnipperJobs::new();
        let request_id = jobs.begin_screen_capture().unwrap_or_default();
        assert!(!jobs.finish_screen_capture(request_id.saturating_add(1)));
        assert!(jobs.has_pending_screen_capture());
        assert!(jobs.finish_screen_capture(request_id));
    }

    #[test]
    fn test_cancel_screen_capture_discards_pending_request() {
        let mut jobs = SnipperJobs::new();
        let request_id = jobs.begin_screen_capture().unwrap_or_default();

        jobs.cancel_screen_capture();

        assert!(!jobs.has_pending_screen_capture());
        assert!(!jobs.finish_screen_capture(request_id));
    }
}
