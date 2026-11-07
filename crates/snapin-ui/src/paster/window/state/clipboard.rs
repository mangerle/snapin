//! 贴图窗口剪贴板请求状态与完成事件处理。

use super::PasterWindowState;
use crate::paster::{render_color_swatch_card, render_simple_text_card, try_parse_hex_color};
use crate::ports::{
    ClipboardJobId, ClipboardPayload, ClipboardResult, ClipboardTarget, ClipboardWrite, PortError,
};
use image::RgbaImage;
use tiny_skia::Pixmap;
use winit::window::Window;

/// 当前剪贴板任务完成后要执行的语义动作。
#[derive(Debug)]
pub(crate) enum PasterClipboardAction {
    /// 复制当前贴图图像；可选在真实复制成功后关闭窗口。
    CopyImage { close_on_success: bool },
    /// 用当前系统剪贴板内容替换贴图图像。
    ReplaceFromClipboard,
    /// 复制辅助文本，真实成功后只展示提示。
    TransientText {
        /// 成功提示。
        success_message: String,
        /// 失败提示前缀。
        failure_label: &'static str,
    },
    /// 读取剪贴板纯文本并插入到文本标注会话中。
    PasteIntoTextInput,
}

/// 当前在途剪贴板任务。
#[derive(Debug)]
pub(crate) struct PendingClipboardJob {
    id: ClipboardJobId,
    action: PasterClipboardAction,
}

impl PasterWindowState {
    /// 是否存在尚未完成的剪贴板任务。
    pub(crate) fn has_pending_clipboard(&self) -> bool {
        self.pending_clipboard.is_some()
    }

    /// 返回当前在途剪贴板任务标识，供测试精确匹配。
    #[cfg(test)]
    pub(crate) fn pending_clipboard_id(&self) -> Option<ClipboardJobId> {
        self.pending_clipboard.as_ref().map(|pending| pending.id)
    }

    /// 提交当前贴图图像复制。
    pub(crate) fn copy_image_to_clipboard(&mut self, close_on_success: bool) -> bool {
        let Some(image) = self.paster_image_for_clipboard() else {
            return false;
        };
        self.submit_clipboard_write(
            ClipboardWrite::Image(image),
            PasterClipboardAction::CopyImage { close_on_success },
        )
    }

    /// 提交辅助文本复制，成功后展示指定提示。
    pub(crate) fn copy_text_to_clipboard(
        &mut self,
        text: String,
        success_message: String,
        failure_label: &'static str,
    ) -> bool {
        self.submit_clipboard_write(
            ClipboardWrite::Text(text),
            PasterClipboardAction::TransientText {
                success_message,
                failure_label,
            },
        )
    }

    /// 请求读取当前系统剪贴板并替换贴图内容。
    pub(crate) fn request_clipboard_replace(&mut self) -> bool {
        self.submit_clipboard_request(PasterClipboardAction::ReplaceFromClipboard)
    }

    /// 取消当前在途剪贴板任务。
    pub(crate) fn cancel_pending_clipboard(&mut self) {
        self.pending_clipboard = None;
    }

    /// 处理剪贴板任务完成事件；返回是否匹配当前在途任务。
    pub(crate) fn handle_clipboard_finished(
        &mut self,
        job_id: ClipboardJobId,
        result: Result<ClipboardResult, PortError>,
        window: Option<&Window>,
    ) -> bool {
        let Some(pending) = self.pending_clipboard.take() else {
            return false;
        };
        if pending.id != job_id {
            self.pending_clipboard = Some(pending);
            return false;
        }

        match pending.action {
            PasterClipboardAction::CopyImage { close_on_success } => {
                self.handle_copy_image_finished(result, close_on_success);
            }
            PasterClipboardAction::TransientText {
                success_message,
                failure_label,
            } => {
                self.handle_transient_text_finished(result, success_message, failure_label);
            }
            PasterClipboardAction::ReplaceFromClipboard => {
                self.handle_replace_finished(result, window);
            }
            PasterClipboardAction::PasteIntoTextInput => {
                if let Ok(ClipboardResult::Payload(ClipboardPayload::Text(text))) = result {
                    if let Some(session) = &mut self.interaction.annotation.active_text_input {
                        session.insert_str(&text);
                    }
                }
            }
        }
        true
    }

    /// 请求读取系统剪贴板纯文本并插入到文本标注会话中。
    pub(crate) fn request_clipboard_text_for_input(&mut self) -> bool {
        self.submit_clipboard_request(PasterClipboardAction::PasteIntoTextInput)
    }

    /// 处理图像复制结果。
    fn handle_copy_image_finished(
        &mut self,
        result: Result<ClipboardResult, PortError>,
        close_on_success: bool,
    ) {
        match result {
            Ok(ClipboardResult::Written) => {
                self.set_status_message(snapin_foundation::i18n::tr("已复制图像到剪贴板"));
                self.services.desktop.notify_copy("贴图图像已复制到剪贴板");
                if close_on_success {
                    self.should_close = true;
                }
            }
            Ok(ClipboardResult::Payload(_)) => {
                self.set_status_message("复制图像失败: 宿主返回了不匹配的剪贴板结果");
            }
            Err(error) => self.set_status_message(format!("复制图像失败: {error}")),
        }
    }

    /// 处理临时文本复制结果。
    fn handle_transient_text_finished(
        &mut self,
        result: Result<ClipboardResult, PortError>,
        success_message: String,
        failure_label: &'static str,
    ) {
        match result {
            Ok(ClipboardResult::Written) => self.set_status_message(success_message),
            Ok(ClipboardResult::Payload(_)) => {
                self.set_status_message("复制文本失败: 宿主返回了不匹配的剪贴板结果");
            }
            Err(error) => self.set_status_message(format!("{failure_label}失败: {error}")),
        }
    }

    /// 处理从剪贴板替换贴图内容的结果。
    fn handle_replace_finished(
        &mut self,
        result: Result<ClipboardResult, PortError>,
        window: Option<&Window>,
    ) {
        match result {
            Ok(ClipboardResult::Payload(payload)) => match window {
                Some(window) => self.apply_clipboard_payload(payload, window),
                None => self.set_status_message("应用剪贴板内容失败: 目标贴图窗口已关闭"),
            },
            Ok(ClipboardResult::Written) => {
                self.set_status_message("读取剪贴板失败: 宿主返回了不匹配的剪贴板结果");
            }
            Err(error) => self.set_status_message(format!("读取剪贴板失败: {error}")),
        }
    }

    /// 将当前贴图转换为可用于系统剪贴板的 RGBA 图像。
    fn paster_image_for_clipboard(&mut self) -> Option<RgbaImage> {
        match super::super::jobs::pixmap_to_rgba_image(&self.document.item.pixmap) {
            Ok(image) => Some(image),
            Err(message) => {
                self.set_status_message(format!("读取贴图像素失败，无法复制: {message}"));
                None
            }
        }
    }

    /// 提交剪贴板写入并登记后续动作。
    fn submit_clipboard_write(
        &mut self,
        write: ClipboardWrite,
        action: PasterClipboardAction,
    ) -> bool {
        if self.has_pending_clipboard() {
            self.set_status_message(snapin_foundation::i18n::tr("请等待当前剪贴板任务完成"));
            return false;
        }

        let target = ClipboardTarget::Paster(self.document.item.id);
        let events = self.services.clipboard_events.clone();
        let sink = Box::new(move |job_id, result| {
            events(target, job_id, result);
        });
        match self.services.desktop.write_clipboard(write, sink) {
            Ok(id) => {
                self.pending_clipboard = Some(PendingClipboardJob { id, action });
                true
            }
            Err(error) => {
                self.set_status_message(format!("提交剪贴板写入任务失败: {error}"));
                false
            }
        }
    }

    /// 提交剪贴板读取并登记后续动作。
    fn submit_clipboard_request(&mut self, action: PasterClipboardAction) -> bool {
        if self.has_pending_clipboard() {
            self.set_status_message(snapin_foundation::i18n::tr("请等待当前剪贴板任务完成"));
            return false;
        }

        let target = ClipboardTarget::Paster(self.document.item.id);
        let events = self.services.clipboard_events.clone();
        let sink = Box::new(move |job_id, result| {
            events(target, job_id, result);
        });
        match self.services.desktop.read_clipboard(sink) {
            Ok(id) => {
                self.pending_clipboard = Some(PendingClipboardJob { id, action });
                true
            }
            Err(error) => {
                self.set_status_message(format!("提交剪贴板读取任务失败: {error}"));
                false
            }
        }
    }

    /// 应用读取到的剪贴板载荷。
    fn apply_clipboard_payload(&mut self, payload: ClipboardPayload, window: &Window) {
        match payload {
            ClipboardPayload::Image(image) => {
                let Some(mut pixmap) = Pixmap::new(image.width(), image.height()) else {
                    self.set_status_message("分配剪贴板图像缓冲区失败");
                    return;
                };
                pixmap.data_mut().copy_from_slice(image.as_raw());
                self.replace_pixmap(pixmap, window);
            }
            ClipboardPayload::Text(text) => {
                let pixmap = if let Some((r, g, b)) = try_parse_hex_color(&text) {
                    render_color_swatch_card(r, g, b)
                } else {
                    let lines: Vec<&str> = text.lines().collect();
                    render_simple_text_card(&lines)
                };
                if let Some(pixmap) = pixmap {
                    self.replace_pixmap(pixmap, window);
                } else {
                    self.set_status_message("生成剪贴板文本卡片失败");
                }
            }
            ClipboardPayload::Empty => {
                self.set_status_message(snapin_foundation::i18n::tr("当前剪贴板为空"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PasterItem;
    use crate::ports::PortErrorKind;
    use tiny_skia::Pixmap;

    /// 构造最小贴图状态。
    fn test_state() -> PasterWindowState {
        let pixmap = Pixmap::new(2, 2).expect("创建测试位图失败");
        PasterWindowState::new(PasterItem::new(1, "测试贴图".to_string(), pixmap, (0, 0)))
    }

    /// “复制并关闭”必须等待真实剪贴板写入成功，不能提交后立即关闭。
    #[test]
    fn test_copy_and_close_waits_for_real_write_result() {
        let mut state = test_state();
        assert!(state.copy_image_to_clipboard(true));
        assert!(!state.should_close, "任务入队不得视为复制成功");

        let job_id = state.pending_clipboard_id().expect("应登记在途剪贴板任务");
        assert!(state.handle_clipboard_finished(job_id, Ok(ClipboardResult::Written), None));
        assert!(state.should_close, "真实写入成功后应执行关闭动作");
    }

    /// 真实写入失败时必须保持窗口打开并展示错误。
    #[test]
    fn test_copy_failure_keeps_window_open() {
        let mut state = test_state();
        assert!(state.copy_image_to_clipboard(true));
        let job_id = state.pending_clipboard_id().expect("应登记在途剪贴板任务");
        assert!(state.handle_clipboard_finished(
            job_id,
            Err(PortError::with_kind(
                PortErrorKind::Unavailable,
                "写入系统剪贴板",
                "剪贴板不可用",
            )),
            None
        ));
        assert!(!state.should_close, "复制失败不得关闭窗口");
        assert!(state
            .status_message
            .as_ref()
            .map(|(message, _)| message)
            .is_some_and(|message| message.contains("剪贴板不可用")));
    }
}
