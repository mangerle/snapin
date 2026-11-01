//! 贴图任务标识与渲染帧缓冲生命周期。
//!
//! 这些方法只读写状态内部字段，放在 state 子模块中既保持单一职责，
//! 又允许父模块继续对结构体字段保持私有。

use super::PasterWindowState;
use crate::ports::{JobCancel, JobId};
use tiny_skia::Pixmap;

impl PasterWindowState {
    /// 取出与窗口尺寸匹配的渲染帧缓冲；尺寸变化时才重新分配。
    ///
    /// 渲染期间状态仍需要读取图像、工具栏与菜单字段，因此缓冲区先移出状态，
    /// 渲染完成后由调用方归还，避免长期持有 `&mut self` 造成的借用冲突。
    pub(crate) fn take_frame_pixmap(&mut self, width: u32, height: u32) -> Option<Pixmap> {
        let needs_resize = self
            .frame_pixmap
            .as_ref()
            .is_none_or(|pixmap| pixmap.width() != width || pixmap.height() != height);
        if needs_resize {
            self.frame_pixmap = Pixmap::new(width, height);
        }
        self.frame_pixmap.take()
    }

    /// 归还渲染帧缓冲，供下一帧复用。
    pub(crate) fn restore_frame_pixmap(&mut self, pixmap: Pixmap) {
        self.frame_pixmap = Some(pixmap);
    }

    /// 是否有后台媒体任务正在执行。
    pub(crate) fn has_pending_job(&self) -> bool {
        self.pending_job_id.is_some()
    }

    /// 记录新提交的后台任务 ID 与取消闭包。
    pub(crate) fn begin_job(&mut self, job_id: JobId, cancel: JobCancel) {
        self.pending_job_id = Some(job_id);
        self.pending_job_cancel = Some(cancel);
    }

    /// 取消当前后台任务；窗口关闭或用户主动放弃时调用。
    pub(crate) fn cancel_pending_job(&mut self) {
        if let Some(cancel) = self.pending_job_cancel.take() {
            cancel();
        }
        self.pending_job_id = None;
    }

    /// 结束匹配任务；过期任务返回 false，由调用方丢弃结果。
    pub(crate) fn finish_job(&mut self, job_id: JobId) -> bool {
        if self.pending_job_id == Some(job_id) {
            self.pending_job_id = None;
            self.pending_job_cancel = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PasterItem;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tiny_skia::Pixmap;

    /// 关闭窗口时必须真正触发协作式取消，而不是只清掉任务 ID。
    #[test]
    fn test_cancel_pending_job_invokes_cancel_closure() {
        let pixmap = Pixmap::new(2, 2).expect("创建测试位图失败");
        let item = PasterItem::new(1, "测试".to_string(), pixmap, (0, 0));
        let mut state = PasterWindowState::new(item);
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        state.begin_job(
            JobId::new(7),
            Box::new(move || flag.store(true, Ordering::Release)),
        );

        state.cancel_pending_job();
        assert!(cancelled.load(Ordering::Acquire));
        assert!(!state.has_pending_job());
    }
}
