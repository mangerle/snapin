//! 贴图窗口后台任务槽：同一时刻最多一个 OCR/翻译/保存任务。
//!
//! 任务 ID 用于丢弃过期回调，取消闭包确保窗口关闭时能真正停止在途工作；
//! 两者必须一起维护，避免只清 ID 而让任务继续占用推理与网络资源。

use crate::ports::{JobCancel, JobId};

/// 单任务槽状态。
#[derive(Default)]
pub(crate) struct PasterJobSlot {
    pending: Option<PendingPasterJob>,
}

/// 一个已提交但尚未完成的贴图任务。
struct PendingPasterJob {
    id: JobId,
    cancel: JobCancel,
}

impl PasterJobSlot {
    /// 是否有后台任务正在执行。
    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// 记录新提交的任务与取消闭包。
    pub(crate) fn begin(&mut self, job_id: JobId, cancel: JobCancel) {
        self.pending = Some(PendingPasterJob { id: job_id, cancel });
    }

    /// 结束匹配任务；过期任务返回 false，由调用方丢弃结果。
    pub(crate) fn finish(&mut self, job_id: JobId) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.id == job_id)
        {
            self.pending = None;
            true
        } else {
            false
        }
    }

    /// 取消当前任务并清空槽位。
    pub(crate) fn cancel(&mut self) {
        if let Some(pending) = self.pending.take() {
            (pending.cancel)();
        }
    }
}
