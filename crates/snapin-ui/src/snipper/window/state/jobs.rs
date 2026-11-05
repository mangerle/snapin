//! 截图会话的异步媒体与剪贴板任务状态。

use super::super::clipboard::PendingClipboardJob;
use super::super::jobs::{SnipperJobKind, SnipperJobRuntime};
use crate::ports::{noop_clipboard_event_sink, ClipboardEventSink, JobCancel, JobId};
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
}

impl SnipperJobs {
    /// 构建没有在途任务的任务状态。
    pub(crate) fn new() -> Self {
        Self {
            runtime: None,
            pending_media: None,
            pending_clipboard: None,
            clipboard_events: noop_clipboard_event_sink(),
        }
    }
}
