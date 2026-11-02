//! 截图智能打码、二维码与 OCR 面板状态。

use super::super::ocr_panel::OcrPanelState;
use super::super::redaction::RedactionPreview;
use crate::ports::QrCodeItem;

/// OCR、二维码与智能打码结果状态。
#[derive(Default)]
pub(crate) struct SnipperOcr {
    /// 智能打码可点击文字区域。
    pub(crate) redaction_preview: Option<RedactionPreview>,
    /// 最近一次二维码识别结果。
    pub(crate) qr_result: Option<Vec<QrCodeItem>>,
    /// OCR 结果面板。
    pub(crate) panel: Option<OcrPanelState>,
    /// 当前正在编辑的 OCR 面板行。
    pub(crate) edit_row: Option<usize>,
}
