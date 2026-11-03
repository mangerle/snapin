//! 截图会话剪贴板写入状态与完成事件处理。

use super::state::SnipperState;
use super::types::SnipperOutcome;
use crate::ports::{ClipboardJobId, ClipboardResult, ClipboardTarget, ClipboardWrite, PortError};
use image::RgbaImage;

/// 剪贴板写入成功后的截图会话语义。
#[derive(Debug)]
pub(crate) enum SnipperClipboardAction {
    /// 复制选区图像并退出截图会话。
    CopyImage,
    /// 复制取色结果并退出截图会话。
    PickColor(String),
    /// 复制 OCR 全文并退出截图会话。
    CopyText(String),
    /// 复制辅助文本，只展示成功提示并保持截图会话打开。
    TransientStatus {
        /// 真实写入成功后展示的提示。
        success_message: String,
        /// 写入失败时用于定位动作的中文名称。
        failure_label: &'static str,
    },
}

impl SnipperClipboardAction {
    /// 失败提示前缀。
    fn failure_label(&self) -> &'static str {
        match self {
            Self::CopyImage => "复制选区图像",
            Self::PickColor(_) => "复制颜色值",
            Self::CopyText(_) => "复制识别文字",
            Self::TransientStatus { failure_label, .. } => failure_label,
        }
    }
}

/// 当前在途剪贴板任务。
#[derive(Debug)]
pub(crate) struct PendingClipboardJob {
    id: ClipboardJobId,
    action: SnipperClipboardAction,
}

impl SnipperState {
    /// 是否存在尚未完成的剪贴板写入。
    pub(crate) fn has_pending_clipboard(&self) -> bool {
        self.jobs.pending_clipboard.is_some()
    }

    /// 返回当前在途剪贴板任务标识，供测试精确匹配。
    #[cfg(test)]
    pub(crate) fn pending_clipboard_id(&self) -> Option<ClipboardJobId> {
        self.jobs
            .pending_clipboard
            .as_ref()
            .map(|pending| pending.id)
    }

    /// 提交图像复制；任务完成前保持截图会话打开。
    pub(crate) fn submit_clipboard_image(
        &mut self,
        image: RgbaImage,
        action: SnipperClipboardAction,
    ) -> bool {
        self.submit_clipboard_write(ClipboardWrite::Image(image), action)
    }

    /// 提交文本复制；任务完成前保持截图会话打开。
    pub(crate) fn submit_clipboard_text(
        &mut self,
        text: String,
        action: SnipperClipboardAction,
    ) -> bool {
        self.submit_clipboard_write(ClipboardWrite::Text(text), action)
    }

    /// 提交剪贴板写入并登记待完成动作。
    fn submit_clipboard_write(
        &mut self,
        write: ClipboardWrite,
        action: SnipperClipboardAction,
    ) -> bool {
        if self.has_pending_clipboard() {
            self.set_status_message(snapin_foundation::i18n::tr("请等待当前复制任务完成"));
            return false;
        }

        let events = self.jobs.clipboard_events.clone();
        let sink = Box::new(move |job_id, result| {
            events(ClipboardTarget::Snipper, job_id, result);
        });
        match self.session.desktop.write_clipboard(write, sink) {
            Ok(id) => {
                self.jobs.pending_clipboard = Some(PendingClipboardJob { id, action });
                true
            }
            Err(error) => {
                self.set_status_message(format!("{}失败: {error}", action.failure_label()));
                false
            }
        }
    }

    /// 处理剪贴板写入完成事件；返回是否匹配当前在途任务。
    pub(crate) fn handle_clipboard_finished(
        &mut self,
        job_id: ClipboardJobId,
        result: Result<ClipboardResult, PortError>,
    ) -> bool {
        let Some(pending) = self.jobs.pending_clipboard.take() else {
            return false;
        };
        if pending.id != job_id {
            self.jobs.pending_clipboard = Some(pending);
            return false;
        }

        match result {
            Ok(ClipboardResult::Written) => match pending.action {
                SnipperClipboardAction::CopyImage => {
                    self.session.outcome = Some(SnipperOutcome::CopiedImage);
                }
                SnipperClipboardAction::PickColor(color) => {
                    self.session.outcome = Some(SnipperOutcome::CopiedColor(color));
                }
                SnipperClipboardAction::CopyText(text) => {
                    self.session.outcome = Some(SnipperOutcome::CopiedText(text));
                }
                SnipperClipboardAction::TransientStatus {
                    success_message, ..
                } => {
                    self.set_status_message(success_message);
                }
            },
            Ok(ClipboardResult::Payload(_)) => {
                self.set_status_message(format!(
                    "{}失败: 返回了不匹配的剪贴板结果",
                    pending.action.failure_label()
                ));
            }
            Err(error) => {
                self.set_status_message(format!("{}失败: {error}", pending.action.failure_label()));
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snipper::window::state::SnipperState;
    use image::RgbaImage;

    /// 图像复制必须先登记在途任务，真实写入完成后才设置最终结果。
    #[test]
    fn test_copy_image_waits_for_real_write_result() {
        let mut state = SnipperState::new();
        assert!(
            state.submit_clipboard_image(RgbaImage::new(2, 2), SnipperClipboardAction::CopyImage)
        );
        assert!(
            state.session.outcome.is_none(),
            "任务入队不得提前结束截图会话"
        );

        let job_id = state.pending_clipboard_id().expect("应登记在途剪贴板任务");
        assert!(state.handle_clipboard_finished(job_id, Ok(ClipboardResult::Written)));
        assert!(matches!(
            state.session.outcome,
            Some(SnipperOutcome::CopiedImage)
        ));
    }

    /// 辅助文本复制完成后只展示提示并保持截图会话打开。
    #[test]
    fn test_transient_text_status_keeps_session_open() {
        let mut state = SnipperState::new();
        assert!(state.submit_clipboard_text(
            "hello".to_string(),
            SnipperClipboardAction::TransientStatus {
                success_message: "文本已复制".to_string(),
                failure_label: "复制文本",
            }
        ));
        let job_id = state.pending_clipboard_id().expect("应登记在途剪贴板任务");
        assert!(state.handle_clipboard_finished(job_id, Ok(ClipboardResult::Written)));

        assert!(state.session.outcome.is_none());
        assert_eq!(state.session.status_message.as_deref(), Some("文本已复制"));
    }

    /// OCR 全文复制必须等真实写入成功后再结束截图会话。
    #[test]
    fn test_copy_text_closes_after_real_write() {
        let mut state = SnipperState::new();
        assert!(state.submit_clipboard_text(
            "识别文字".to_string(),
            SnipperClipboardAction::CopyText("识别文字".to_string())
        ));
        assert!(state.session.outcome.is_none());
        let job_id = state.pending_clipboard_id().expect("应登记在途剪贴板任务");
        assert!(state.handle_clipboard_finished(job_id, Ok(ClipboardResult::Written)));
        assert!(matches!(
            state.session.outcome,
            Some(SnipperOutcome::CopiedText(ref text)) if text == "识别文字"
        ));
    }
}
