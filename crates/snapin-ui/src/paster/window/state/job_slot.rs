//! 贴图窗口后台任务槽：同一时刻最多一个 OCR/翻译/保存任务。
//!
//! 任务 ID 用于丢弃过期回调，取消闭包确保窗口关闭时能真正停止在途工作；
//! 两者必须一起维护，避免只清 ID 而让任务继续占用推理与网络资源。

use crate::ports::{JobCancel, JobId};

/// 单任务槽状态。
#[derive(Default)]
pub(crate) struct PasterJobSlot {
    job_id: Option<JobId>,
    cancel: Option<JobCancel>,
}

impl PasterJobSlot {
    /// 是否有后台任务正在执行。
    pub(crate) fn has_pending(&self) -> bool {
        self.job_id.is_some()
    }

    /// 记录新提交的任务与取消闭包。
    pub(crate) fn begin(&mut self, job_id: JobId, cancel: JobCancel) {
        self.job_id = Some(job_id);
        self.cancel = Some(cancel);
    }

    /// 结束匹配任务；过期任务返回 false，由调用方丢弃结果。
    pub(crate) fn finish(&mut self, job_id: JobId) -> bool {
        if self.job_id == Some(job_id) {
            self.job_id = None;
            self.cancel = None;
            true
        } else {
            false
        }
    }

    /// 取消当前任务并清空槽位。
    pub(crate) fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
        self.job_id = None;
    }
}
